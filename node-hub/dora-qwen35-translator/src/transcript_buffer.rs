#[derive(Debug, Default, Clone)]
pub struct TranscriptBuffer {
    buffer: String,
    stable_buffer: String,
    active_burst_id: Option<i64>,
    active_burst_text: String,
}

impl TranscriptBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn stable_buffer(&self) -> &str {
        &self.stable_buffer
    }

    pub fn active_burst_id(&self) -> Option<i64> {
        self.active_burst_id
    }

    pub fn active_burst_text(&self) -> &str {
        &self.active_burst_text
    }

    pub fn update_from_chunk(&mut self, burst_id: Option<i64>, chunk: &str) -> bool {
        if chunk.is_empty() {
            return false;
        }

        let old_buffer = self.buffer.clone();

        match self.active_burst_id {
            None => {
                self.active_burst_id = burst_id;
                self.active_burst_text.clear();
                self.active_burst_text.push_str(chunk);
            }
            Some(active_id) if Some(active_id) == burst_id => {
                self.active_burst_text.clear();
                self.active_burst_text.push_str(chunk);
            }
            Some(_) => {
                self.seal_active_burst();
                self.active_burst_id = burst_id;
                self.active_burst_text.clear();
                self.active_burst_text.push_str(chunk);
            }
        }

        self.rebuild_views();
        self.buffer != old_buffer
    }

    pub fn seal_active_burst(&mut self) -> bool {
        if self.active_burst_text.is_empty() {
            self.active_burst_id = None;
            self.rebuild_views();
            return false;
        }

        self.stable_buffer.push_str(&self.active_burst_text);
        self.active_burst_text.clear();
        self.active_burst_id = None;
        self.rebuild_views();
        true
    }

    pub fn consume_stable_prefix(&mut self, prefix: &str) -> Result<(), String> {
        if prefix.is_empty() {
            return Ok(());
        }

        if !self.stable_buffer.starts_with(prefix) {
            return Err("consumed prefix is not a prefix of stable_buffer".into());
        }

        self.stable_buffer.drain(..prefix.len());
        self.rebuild_views();
        Ok(())
    }

    pub fn uncommitted_tail(&self) -> String {
        self.buffer.trim().to_string()
    }

    pub fn has_stable_text(&self, min_chars: usize) -> bool {
        self.stable_buffer.trim().chars().count() >= min_chars
    }

    pub fn debug_snapshot(&self) -> String {
        format!(
            "buffer=\n{}\nstable_buffer=\n{}\nactive_burst_id={:?}\nactive_burst_text=\n{}",
            self.buffer, self.stable_buffer, self.active_burst_id, self.active_burst_text,
        )
    }

    fn rebuild_views(&mut self) {
        self.buffer.clear();
        self.buffer.push_str(&self.stable_buffer);
        self.buffer.push_str(&self.active_burst_text);
    }
}

#[cfg(test)]
mod tests {
    use super::TranscriptBuffer;

    #[test]
    fn same_burst_replaces_active_text_instead_of_merging() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(7), "好，我叫鲍月，然后来自。");
        state.update_from_chunk(Some(7), "好，我叫鲍月，然后来自华为，现在也是。");

        assert_eq!(state.buffer(), "好，我叫鲍月，然后来自华为，现在也是。");
        assert_eq!(state.stable_buffer(), "");
        assert_eq!(state.active_burst_text(), "好，我叫鲍月，然后来自华为，现在也是。");
    }

    #[test]
    fn new_burst_seals_previous_burst_into_stable_buffer() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(1), "大家下午好，我叫鲍月。");
        state.update_from_chunk(Some(2), "然后来自华为。");

        assert_eq!(state.stable_buffer(), "大家下午好，我叫鲍月。");
        assert_eq!(state.buffer(), "大家下午好，我叫鲍月。然后来自华为。");
        assert_eq!(state.active_burst_id(), Some(2));
    }

    #[test]
    fn consume_stable_prefix_removes_only_stable_text() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(1), "大家下午好，我叫鲍月。");
        state.update_from_chunk(Some(2), "然后来自华为。");
        state
            .consume_stable_prefix("大家下午好，我叫鲍月。")
            .expect("prefix should be consumed");

        assert_eq!(state.uncommitted_tail(), "然后来自华为。");
        assert_eq!(state.stable_buffer(), "");
        assert_eq!(state.active_burst_text(), "然后来自华为。");
    }

    #[test]
    fn sealing_active_burst_makes_it_available_for_translation() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(3), "今天呢，也是想要去探讨。");
        assert_eq!(state.stable_buffer(), "");
        state.seal_active_burst();
        assert_eq!(state.stable_buffer(), "今天呢，也是想要去探讨。");
    }

    #[test]
    fn debug_snapshot_contains_stable_and_active_state() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(7), "大家下午好，我叫鲍月。");
        let snapshot = state.debug_snapshot();
        assert!(snapshot.contains("stable_buffer=\n"));
        assert!(snapshot.contains("active_burst_id=Some(7)"));
        assert!(snapshot.contains("active_burst_text=\n大家下午好，我叫鲍月。"));
    }
}
