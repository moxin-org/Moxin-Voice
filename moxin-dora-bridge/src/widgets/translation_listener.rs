//! Translation Listener Bridge
//!
//! Listens to translator node output (source_text + translation) and writes
//! streaming/final results to SharedDoraState for consumption by the UI overlay.

use crate::bridge::{BridgeState, DoraBridge};
use crate::data::{DoraData, SentenceUnit, TranslationUpdate};
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

        const MAX_HISTORY: usize = 50;

        // Completed sentence history (source + translation pairs).
        let mut history: Vec<SentenceUnit> = Vec::new();
        // Current in-progress ASR text (not yet translated).
        let mut pending_source_text = String::new();
        // Current source text for the sentence being translated.
        let mut current_source_text = String::new();

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
                                    if let Parameter::String(s) = v { Some(s.clone()) } else { None }
                                })
                                .unwrap_or_else(|| "streaming".to_string());

                            debug!("[TranslationListener] source_text ({}): {}", session_status, &text);

                            {
                                // Normal streaming update of pending display.
                                current_source_text = text.clone();
                                pending_source_text = text;
                                if let Some(ref shared) = shared_state {
                                    shared.translation.set(Some(TranslationUpdate {
                                        history: history.clone(),
                                        pending_source_text: pending_source_text.clone(),
                                    }));
                                }
                            }
                        } else if id == DataId::from("translation".to_owned()) {
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

                            debug!(
                                "[TranslationListener] translation ({}): {}",
                                session_status, &text
                            );

                            if session_status == "complete" {
                                info!(
                                    "[TranslationListener] Translation complete: [{}] -> [{}]",
                                    current_source_text, text
                                );

                                // Add completed sentence to history.
                                history.push(SentenceUnit {
                                    source_text: current_source_text.clone(),
                                    translation: text,
                                });
                                // Cap history size.
                                if history.len() > MAX_HISTORY {
                                    history.remove(0);
                                }
                                // Clear pending since this sentence is done.
                                pending_source_text.clear();
                                current_source_text.clear();

                                if let Some(ref shared) = shared_state {
                                    shared.translation.set(Some(TranslationUpdate {
                                        history: history.clone(),
                                        pending_source_text: String::new(),
                                    }));
                                }
                            } else if session_status == "replace_last" {
                                // Retroactive merge: keep history[N] (the latest sentence) visible
                                // at the bottom, remove history[N-1] (the older sentence), and
                                // update history[N] with the combined source + merged translation.
                                // This way the merged result appears where the user is already
                                // looking (the bottom), rather than disappearing and reappearing
                                // at a higher position.
                                let combined_source = metadata
                                    .parameters
                                    .iter()
                                    .find(|(k, _)| k.as_str() == "combined_source")
                                    .and_then(|(_, v)| {
                                        if let Parameter::String(s) = v { Some(s.clone()) } else { None }
                                    });
                                let last_idx = history.len().saturating_sub(1);
                                if last_idx >= 1 {
                                    // Remove N-1 (the older entry), update N (latest) in-place.
                                    history.remove(last_idx - 1);
                                    if let Some(latest) = history.last_mut() {
                                        if let Some(src) = combined_source {
                                            latest.source_text = src;
                                        }
                                        latest.translation = text.clone();
                                        info!(
                                            "[TranslationListener] replace_last: [{}] -> [{}]",
                                            latest.source_text, text
                                        );
                                    }
                                } else if let Some(only) = history.last_mut() {
                                    // Only one entry — just update it.
                                    if let Some(src) = combined_source {
                                        only.source_text = src;
                                    }
                                    only.translation = text.clone();
                                }
                                pending_source_text.clear();
                                current_source_text.clear();
                                if let Some(ref shared) = shared_state {
                                    shared.translation.set(Some(TranslationUpdate {
                                        history: history.clone(),
                                        pending_source_text: String::new(),
                                    }));
                                }
                            }
                            // Ignore streaming translation chunks — overlay only shows completed translations.
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
