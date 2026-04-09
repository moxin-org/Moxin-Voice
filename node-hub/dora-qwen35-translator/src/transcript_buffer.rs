#[derive(Debug, Default, Clone)]
pub struct TranscriptBuffer {
    buffer: String,
    stable_buffer: String,
    raw_buffer: String,
    raw_prefix_map: Vec<(usize, usize)>,
    committed_raw_pos: usize,
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

    pub fn raw_buffer(&self) -> &str {
        &self.raw_buffer
    }

    pub fn committed_raw_pos(&self) -> usize {
        self.committed_raw_pos
    }

    pub fn active_burst_id(&self) -> Option<i64> {
        self.active_burst_id
    }

    pub fn active_burst_start(&self) -> usize {
        self.stable_buffer.len()
    }

    pub fn active_burst_text(&self) -> &str {
        &self.active_burst_text
    }

    pub fn debug_snapshot(&self) -> String {
        format!(
            "buffer=\n{}\nstable_buffer=\n{}\nraw_buffer=\n{}\ncommitted_raw_pos={}\nactive_burst_id={:?}\nactive_burst_start={}\nactive_burst_text=\n{}",
            self.buffer,
            self.stable_buffer,
            self.raw_buffer,
            self.committed_raw_pos,
            self.active_burst_id,
            self.active_burst_start(),
            self.active_burst_text,
        )
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

    pub fn raw_uncommitted_tail(&self) -> &str {
        if self.committed_raw_pos > self.raw_buffer.len() {
            ""
        } else {
            &self.raw_buffer[self.committed_raw_pos..]
        }
    }

    pub fn has_pending_raw_text(&self, min_chars: usize) -> bool {
        self.raw_uncommitted_tail().chars().count() >= min_chars
    }

    pub fn commit_raw_prefix(&mut self, raw_prefix: &str) -> Result<(), String> {
        if raw_prefix.is_empty() {
            return Ok(());
        }

        let raw_tail = self.raw_uncommitted_tail();
        if !raw_tail.starts_with(raw_prefix) {
            return Err("committed_prefix is not a prefix of raw_uncommitted_tail".into());
        }

        self.committed_raw_pos += raw_prefix.len();
        self.normalize_positions();
        Ok(())
    }

    pub fn uncommitted_tail(&self) -> String {
        let stable_start = self
            .stable_buffer_pos_for_raw_prefix(self.committed_raw_pos)
            .unwrap_or(self.stable_buffer.len());
        let stable_tail = &self.stable_buffer[stable_start..];
        format!("{}{}", stable_tail, self.active_burst_text).trim().to_string()
    }

    fn stable_buffer_pos_for_raw_prefix(&self, raw_end: usize) -> Option<usize> {
        if raw_end == 0 {
            return Some(0);
        }

        self.raw_prefix_map
            .iter()
            .find(|(mapped_raw_end, _)| *mapped_raw_end == raw_end)
            .map(|(_, stable_end)| skip_dropped_chars(&self.stable_buffer, *stable_end))
    }

    fn rebuild_views(&mut self) {
        self.buffer.clear();
        self.buffer.push_str(&self.stable_buffer);
        self.buffer.push_str(&self.active_burst_text);

        let (raw_buffer, raw_prefix_map) = normalize_with_map(&self.stable_buffer);
        self.raw_buffer = raw_buffer;
        self.raw_prefix_map = raw_prefix_map;
        self.normalize_positions();
    }

    fn normalize_positions(&mut self) {
        self.committed_raw_pos = self.committed_raw_pos.min(self.raw_buffer.len());
    }
}

fn should_keep_connector(ch: char, prev: Option<char>, next: Option<char>) -> bool {
    matches!(ch, '+' | '#' | '-' | '_' | '/' | '.')
        && prev.is_some_and(is_ascii_word)
        && next.is_some_and(is_ascii_word)
}

fn is_ascii_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
}

fn should_keep_normalized_char(chars: &[(usize, char)], vec_idx: usize) -> bool {
    let (_, ch) = chars[vec_idx];
    let prev = vec_idx
        .checked_sub(1)
        .and_then(|i| chars.get(i))
        .map(|(_, c)| *c);
    let next = chars.get(vec_idx + 1).map(|(_, c)| *c);

    if ch.is_whitespace() {
        return false;
    }

    !matches!(
        ch,
        '，'
            | '。'
            | '！'
            | '？'
            | '；'
            | '：'
            | '、'
            | '（'
            | '）'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '【'
            | '】'
            | '《'
            | '》'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '…'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | '"'
            | '\''
            | '«'
            | '»'
    ) || should_keep_connector(ch, prev, next)
}

fn skip_dropped_chars(text: &str, start: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (vec_idx, (byte_idx, _)) in chars.iter().copied().enumerate() {
        if byte_idx < start {
            continue;
        }
        if should_keep_normalized_char(&chars, vec_idx) {
            return byte_idx;
        }
    }

    text.len()
}

fn normalize_with_map(text: &str) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut raw = String::new();
    let mut map = Vec::new();

    for (vec_idx, (byte_idx, ch)) in chars.iter().copied().enumerate() {
        if !should_keep_normalized_char(&chars, vec_idx) {
            continue;
        }

        raw.push(ch);
        map.push((raw.len(), byte_idx + ch.len_utf8()));
    }

    (raw, map)
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
        assert_eq!(state.raw_buffer(), "");
    }

    #[test]
    fn new_burst_seals_previous_burst_into_stable_and_raw() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(1), "大家下午好，我叫鲍月。");
        state.update_from_chunk(Some(2), "然后来自华为。");

        assert_eq!(state.stable_buffer(), "大家下午好，我叫鲍月。");
        assert_eq!(state.buffer(), "大家下午好，我叫鲍月。然后来自华为。");
        assert_eq!(state.raw_buffer(), "大家下午好我叫鲍月");
        assert_eq!(state.active_burst_id(), Some(2));
    }

    #[test]
    fn commit_raw_prefix_only_advances_stable_raw_position() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(1), "大家下午好，我叫鲍月。");
        state.update_from_chunk(Some(2), "然后来自华为。");
        state
            .commit_raw_prefix("大家下午好我叫鲍月")
            .expect("prefix should commit");

        assert_eq!(state.committed_raw_pos(), "大家下午好我叫鲍月".len());
        assert_eq!(state.raw_uncommitted_tail(), "");
        assert_eq!(state.uncommitted_tail(), "然后来自华为。");
    }

    #[test]
    fn sealing_active_burst_makes_it_available_for_translation() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(3), "今天呢，也是想要去探讨。");
        assert_eq!(state.raw_buffer(), "");
        state.seal_active_burst();
        assert_eq!(state.raw_buffer(), "今天呢也是想要去探讨");
    }

    #[test]
    fn raw_buffer_normalization_preserves_technical_terms() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(1), "Today, we use C++, Rust, HTTP/2, and node.js.");
        state.seal_active_burst();
        assert_eq!(state.raw_buffer(), "TodayweuseC++RustHTTP/2andnode.js");
    }

    #[test]
    fn debug_snapshot_contains_stable_and_active_state() {
        let mut state = TranscriptBuffer::new();
        state.update_from_chunk(Some(7), "大家下午好，我叫鲍月。");
        let snapshot = state.debug_snapshot();
        assert!(snapshot.contains("stable_buffer=\n"));
        assert!(snapshot.contains("raw_buffer=\n"));
        assert!(snapshot.contains("committed_raw_pos=0"));
        assert!(snapshot.contains("active_burst_id=Some(7)"));
        assert!(snapshot.contains("active_burst_text=\n大家下午好，我叫鲍月。"));
    }
}
