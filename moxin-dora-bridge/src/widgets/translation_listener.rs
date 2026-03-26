//! Translation Listener Bridge
//!
//! Listens to dora-qwen3-translator output (source_text + translation) and writes
//! streaming/final results to SharedDoraState for consumption by the UI overlay.

use crate::bridge::{BridgeState, DoraBridge};
use crate::data::{DoraData, TranslationUpdate};
use crate::error::{BridgeError, BridgeResult};
use crate::shared_state::SharedDoraState;
use crossbeam_channel::{bounded, Receiver, Sender};
use dora_node_api::{
    dora_core::config::{DataId, NodeId},
    DoraNode, Event, Parameter,
};
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use tracing::{debug, error, info, warn};

/// Translation Listener Bridge - monitors translator node output
pub struct TranslationListenerBridge {
    /// Node ID in the dataflow (should be "moxin-translation-listener")
    node_id: String,
    /// Connection state
    state: Arc<RwLock<BridgeState>>,
    /// Shared state for writing translation results
    shared_state: Option<Arc<SharedDoraState>>,
    /// Stop signal
    stop_sender: Option<Sender<()>>,
    /// Worker thread handle
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl TranslationListenerBridge {
    /// Create a new translation listener bridge
    pub fn new(node_id: &str) -> Self {
        Self::with_shared_state(node_id, None)
    }

    /// Create with shared state
    pub fn with_shared_state(node_id: &str, shared_state: Option<Arc<SharedDoraState>>) -> Self {
        Self {
            node_id: node_id.to_string(),
            state: Arc::new(RwLock::new(BridgeState::Disconnected)),
            shared_state,
            stop_sender: None,
            worker_handle: None,
        }
    }

    /// Worker thread: listens for source_text and translation events from the translator node.
    ///
    /// The translator node sends:
    /// - `source_text`: the original ASR transcription (plain string, once per sentence)
    /// - `translation`: token batch with metadata `session_status = "streaming" | "complete"`
    ///
    /// We accumulate source_text in a local buffer, updating translation with each batch.
    fn run_event_loop(
        node_id: String,
        state: Arc<RwLock<BridgeState>>,
        shared_state: Option<Arc<SharedDoraState>>,
        stop_receiver: Receiver<()>,
    ) {
        info!("[TranslationListener] Worker started for node: {}", node_id);

        let (mut _node, mut events) =
            match DoraNode::init_from_node_id(NodeId::from(node_id.clone())) {
                Ok(n) => n,
                Err(e) => {
                    error!("[TranslationListener] Failed to init dora node: {}", e);
                    *state.write() = BridgeState::Error;
                    return;
                }
            };

        info!("[TranslationListener] Connected to dora as: {}", node_id);
        *state.write() = BridgeState::Connected;

        if let Some(ref shared) = shared_state {
            shared.add_bridge(node_id.clone());
        }

        // Local buffer: keeps the latest source_text for the current sentence.
        // The translator sends source_text once, then possibly multiple translation chunks.
        let mut current_source_text = String::new();
        // Accumulated translation text (built up from streaming chunks).
        let mut accumulated_translation = String::new();
        // Previous completed sentence — carried in every SharedDoraState update so
        // the UI overlay can display it even if it missed the is_complete transition.
        let mut prev_source_text = String::new();
        let mut prev_translation = String::new();

        loop {
            if stop_receiver.try_recv().is_ok() {
                info!("[TranslationListener] Received stop signal");
                break;
            }

            if let Some(event) = events.recv_timeout(std::time::Duration::from_millis(100)) {
                match event {
                    Event::Input { id, metadata, data } => {
                        // Extract text value from Arrow StringArray
                        let text_value: Option<String> = {
                            use arrow::array::Array;
                            data.as_any()
                                .downcast_ref::<arrow::array::StringArray>()
                                .and_then(|arr| {
                                    if arr.len() > 0 {
                                        Some(arr.value(0).to_string())
                                    } else {
                                        None
                                    }
                                })
                        };

                        let text = match text_value {
                            Some(t) => t,
                            None => continue,
                        };

                        if id == DataId::from("log".to_owned()) {
                            eprintln!("[TranslatorLog] {}", text);
                        } else if id == DataId::from("source_text".to_owned()) {
                            let session_status = metadata
                                .parameters
                                .iter()
                                .find(|(k, _)| k.as_str() == "session_status")
                                .and_then(|(_, v)| {
                                    if let Parameter::String(s) = v {
                                        Some(s.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or("complete");

                            // New sentence / partial ASR streaming update.
                            debug!(
                                "[TranslationListener] source_text (status={}): {}",
                                session_status, &text
                            );

                            // If source text changed (new sentence starting) and we had
                            // a non-empty translation for the old sentence, promote it
                            // to prev even if we never saw is_complete (handles the case
                            // where complete was overwritten before UI polled).
                            if text != current_source_text
                                && !current_source_text.is_empty()
                                && !accumulated_translation.is_empty()
                            {
                                prev_source_text = current_source_text.clone();
                                prev_translation = accumulated_translation.clone();
                            }

                            current_source_text = text;
                            accumulated_translation.clear();

                            // Push source-only update so overlay can render ASR stream and
                            // show "working" state before translation tokens arrive.
                            if let Some(ref shared) = shared_state {
                                shared.translation.set(Some(TranslationUpdate {
                                    source_text: current_source_text.clone(),
                                    translation: String::new(),
                                    is_complete: false,
                                    prev_source_text: prev_source_text.clone(),
                                    prev_translation: prev_translation.clone(),
                                }));
                            }
                        } else if id == DataId::from("translation".to_owned()) {
                            // Read session_status from metadata parameters
                            let session_status = metadata
                                .parameters
                                .iter()
                                .find(|(k, _)| k.as_str() == "session_status")
                                .and_then(|(_, v)| {
                                    if let Parameter::String(s) = v {
                                        Some(s.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or("complete");

                            let is_complete = session_status == "complete";
                            if is_complete {
                                // "complete" payload from translator is the FULL sentence.
                                // Replace instead of append, otherwise we duplicate content.
                                accumulated_translation = text.clone();
                            } else {
                                // "streaming" payload is a delta chunk.
                                accumulated_translation.push_str(&text);
                            }

                            debug!(
                                "[TranslationListener] translation chunk (complete={}): {}",
                                is_complete, &text
                            );

                            if let Some(ref shared) = shared_state {
                                shared.translation.set(Some(TranslationUpdate {
                                    source_text: current_source_text.clone(),
                                    translation: accumulated_translation.clone(),
                                    is_complete,
                                    prev_source_text: prev_source_text.clone(),
                                    prev_translation: prev_translation.clone(),
                                }));
                            }

                            if is_complete {
                                info!(
                                    "[TranslationListener] Translation complete: [{}] -> [{}]",
                                    current_source_text, accumulated_translation
                                );
                                eprintln!(
                                    "[TranslationListener] complete source=[{}] translation=[{}]",
                                    current_source_text, accumulated_translation
                                );
                                // Promote completed sentence to prev for next round.
                                prev_source_text = current_source_text.clone();
                                prev_translation = accumulated_translation.clone();
                                // Reset for next sentence
                                accumulated_translation.clear();
                            }
                        }
                    }
                    Event::Stop(_) => {
                        info!("[TranslationListener] Received stop event from dora");
                        break;
                    }
                    Event::InputClosed { id } => {
                        debug!("[TranslationListener] Input closed: {:?}", id);
                    }
                    Event::Error(e) => {
                        error!("[TranslationListener] Dora error: {}", e);
                    }
                    _ => {}
                }
            }
        }

        info!("[TranslationListener] Worker stopped");
        *state.write() = BridgeState::Disconnected;

        if let Some(ref shared) = shared_state {
            shared.remove_bridge(&node_id);
        }
    }
}

impl DoraBridge for TranslationListenerBridge {
    fn node_id(&self) -> &str {
        &self.node_id
    }

    fn state(&self) -> BridgeState {
        *self.state.read()
    }

    fn connect(&mut self) -> BridgeResult<()> {
        if self.is_connected() {
            return Ok(());
        }

        *self.state.write() = BridgeState::Connecting;

        let (stop_tx, stop_rx) = bounded(1);
        self.stop_sender = Some(stop_tx);

        let node_id = self.node_id.clone();
        let state = Arc::clone(&self.state);
        let shared_state = self.shared_state.clone();

        let handle = thread::Builder::new()
            .name(format!("translation-listener-{}", node_id))
            .spawn(move || {
                Self::run_event_loop(node_id, state, shared_state, stop_rx);
            })
            .map_err(|e| BridgeError::ThreadSpawnFailed(e.to_string()))?;

        self.worker_handle = Some(handle);

        // Wait for connection — macOS Unix socket init is slower
        #[cfg(target_os = "macos")]
        let max_wait_iterations = 100; // 10 seconds
        #[cfg(not(target_os = "macos"))]
        let max_wait_iterations = 50; // 5 seconds

        for i in 0..max_wait_iterations {
            std::thread::sleep(std::time::Duration::from_millis(100));
            match *self.state.read() {
                BridgeState::Connected => {
                    info!(
                        "[TranslationListener] Connection verified after {} ms",
                        i * 100
                    );
                    return Ok(());
                }
                BridgeState::Error => {
                    error!(
                        "[TranslationListener] Bridge failed to connect: {}",
                        self.node_id
                    );
                    return Err(BridgeError::ConnectionFailed(format!(
                        "TranslationListener {} failed to init dora node",
                        self.node_id
                    )));
                }
                _ => {}
            }
        }

        warn!(
            "[TranslationListener] Bridge connection timeout for: {}",
            self.node_id
        );
        Err(BridgeError::ConnectionFailed(format!(
            "TranslationListener {} connection timeout",
            self.node_id
        )))
    }

    fn disconnect(&mut self) -> BridgeResult<()> {
        if let Some(stop_tx) = self.stop_sender.take() {
            let _ = stop_tx.send(());
        }

        if let Some(handle) = self.worker_handle.take() {
            handle.join().map_err(|_| BridgeError::ThreadJoinFailed)?;
        }

        *self.state.write() = BridgeState::Disconnected;
        Ok(())
    }

    fn send(&self, _output: &str, _data: DoraData) -> BridgeResult<()> {
        // This bridge only listens, doesn't send
        Err(BridgeError::NotSupported(
            "TranslationListenerBridge does not support sending data".to_string(),
        ))
    }

    fn expected_inputs(&self) -> Vec<String> {
        vec![
            "source_text".to_string(),
            "translation".to_string(),
            "log".to_string(),
        ]
    }

    fn expected_outputs(&self) -> Vec<String> {
        vec![]
    }
}
