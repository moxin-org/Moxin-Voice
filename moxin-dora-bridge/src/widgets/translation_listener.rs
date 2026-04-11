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
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Default)]
struct TranslationDisplayState {
    history: Vec<SentenceUnit>,
    pending_source_text: String,
    current_source_text: String,
    pending_completed_sources: HashMap<i64, String>,
    pending_completed_translations: HashMap<i64, String>,
    finalized_commit_ids: HashSet<i64>,
}

impl TranslationDisplayState {
    fn handle_source_text(
        &mut self,
        session_status: &str,
        text: String,
        commit_id: Option<i64>,
        max_history: usize,
    ) -> bool {
        match session_status {
            "streaming" => {
                self.current_source_text = text.clone();
                self.pending_source_text = text;
                false
            }
            "complete" => {
                if let Some(commit_id) = commit_id {
                    if self.finalized_commit_ids.contains(&commit_id) {
                        return false;
                    }
                    self.pending_completed_sources.insert(commit_id, text);
                    self.try_finalize_complete_pair(commit_id, max_history)
                } else {
                    // Legacy fallback if complete source arrives without commit_id.
                    self.current_source_text = text;
                    false
                }
            }
            _ => {
                self.current_source_text = text.clone();
                self.pending_source_text = text;
                false
            }
        }
    }

    fn handle_translation_complete(
        &mut self,
        text: String,
        commit_id: Option<i64>,
        source_text: Option<String>,
        max_history: usize,
    ) -> bool {
        if let Some(commit_id) = commit_id {
            if self.finalized_commit_ids.contains(&commit_id) {
                return false;
            }
            if let Some(source_text) = source_text {
                self.pending_completed_sources.remove(&commit_id);
                self.pending_completed_translations.remove(&commit_id);
                self.finalized_commit_ids.insert(commit_id);
                self.push_completed_sentence(source_text, text, max_history);
                return true;
            }
            self.pending_completed_translations.insert(commit_id, text);
            self.try_finalize_complete_pair(commit_id, max_history)
        } else {
            self.push_completed_sentence(self.current_source_text.clone(), text, max_history);
            true
        }
    }

    fn try_finalize_complete_pair(&mut self, commit_id: i64, max_history: usize) -> bool {
        let Some(source_text) = self.pending_completed_sources.remove(&commit_id) else {
            return false;
        };
        let Some(translation) = self.pending_completed_translations.remove(&commit_id) else {
            self.pending_completed_sources.insert(commit_id, source_text);
            return false;
        };

        self.finalized_commit_ids.insert(commit_id);
        self.push_completed_sentence(source_text, translation, max_history);
        true
    }

    fn push_completed_sentence(
        &mut self,
        source_text: String,
        translation: String,
        max_history: usize,
    ) {
        self.history.push(SentenceUnit {
            source_text: source_text.clone(),
            translation,
        });
        if self.history.len() > max_history {
            self.history.remove(0);
        }

        if self.pending_source_text == source_text {
            self.pending_source_text.clear();
        }
        if self.current_source_text == source_text {
            self.current_source_text.clear();
        }
    }
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
        let mut display = TranslationDisplayState::default();

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
                            let commit_id = metadata
                                .parameters
                                .iter()
                                .find(|(k, _)| k.as_str() == "commit_id")
                                .and_then(|(_, v)| {
                                    if let Parameter::Integer(v) = v {
                                        Some(*v)
                                    } else {
                                        None
                                    }
                                });

                            debug!(
                                "[TranslationListener] source_text ({}, commit_id={:?}): {}",
                                session_status,
                                commit_id,
                                &text
                            );

                            display.handle_source_text(
                                &session_status,
                                text,
                                commit_id,
                                MAX_HISTORY,
                            );
                            if let Some(ref shared) = shared_state {
                                shared.translation.set(Some(TranslationUpdate {
                                    history: display.history.clone(),
                                    pending_source_text: display.pending_source_text.clone(),
                                }));
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
                            let commit_id = metadata
                                .parameters
                                .iter()
                                .find(|(k, _)| k.as_str() == "commit_id")
                                .and_then(|(_, v)| {
                                    if let Parameter::Integer(v) = v {
                                        Some(*v)
                                    } else {
                                        None
                                    }
                                });
                            let source_text_meta = metadata
                                .parameters
                                .iter()
                                .find(|(k, _)| k.as_str() == "source_text")
                                .and_then(|(_, v)| {
                                    if let Parameter::String(v) = v {
                                        Some(v.clone())
                                    } else {
                                        None
                                    }
                                });

                            debug!(
                                "[TranslationListener] translation ({}, commit_id={:?}, source_text_meta_present={}): {}",
                                session_status,
                                commit_id,
                                source_text_meta.is_some(),
                                &text
                            );

                            if session_status == "complete" {
                                let completed = display.handle_translation_complete(
                                    text,
                                    commit_id,
                                    source_text_meta,
                                    MAX_HISTORY,
                                );
                                if completed {
                                    if let Some(latest) = display.history.last() {
                                        info!(
                                            "[TranslationListener] Translation complete: [{}] -> [{}]",
                                            latest.source_text,
                                            latest.translation
                                        );
                                    }
                                }

                                if let Some(ref shared) = shared_state {
                                    shared.translation.set(Some(TranslationUpdate {
                                        history: display.history.clone(),
                                        pending_source_text: display.pending_source_text.clone(),
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

#[cfg(test)]
mod tests {
    use super::TranslationDisplayState;

    #[test]
    fn complete_pairing_uses_commit_id_instead_of_latest_streaming_source() {
        let mut state = TranslationDisplayState::default();

        state.handle_source_text("streaming", "旧句".to_string(), None, 50);
        state.handle_source_text("complete", "旧句".to_string(), Some(1), 50);
        state.handle_source_text("streaming", "新句，后半段".to_string(), None, 50);

        let completed = state.handle_translation_complete(
            "old translation".to_string(),
            Some(1),
            None,
            50,
        );
        assert!(completed);

        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].source_text, "旧句");
        assert_eq!(state.history[0].translation, "old translation");
        assert_eq!(state.pending_source_text, "新句，后半段");
    }

    #[test]
    fn finalized_commit_clears_pending_only_when_pending_matches_same_source() {
        let mut state = TranslationDisplayState::default();

        state.handle_source_text("streaming", "同一句".to_string(), None, 50);
        state.handle_source_text("complete", "同一句".to_string(), Some(7), 50);

        let completed = state.handle_translation_complete(
            "same translation".to_string(),
            Some(7),
            None,
            50,
        );
        assert!(completed);

        assert!(state.pending_source_text.is_empty());
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].source_text, "同一句");
    }

    #[test]
    fn translation_complete_with_embedded_source_finalizes_without_separate_source_event() {
        let mut state = TranslationDisplayState::default();

        state.handle_source_text("streaming", "新句，后半段".to_string(), None, 50);

        let completed = state.handle_translation_complete(
            "old translation".to_string(),
            Some(11),
            Some("旧句".to_string()),
            50,
        );
        assert!(completed);

        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].source_text, "旧句");
        assert_eq!(state.history[0].translation, "old translation");
        assert_eq!(state.pending_source_text, "新句，后半段");

        let completed_again = state.handle_source_text("complete", "旧句".to_string(), Some(11), 50);
        assert!(!completed_again);
        assert_eq!(state.history.len(), 1);
    }
}
