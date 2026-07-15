#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegmentAudioError {
    EmptySegments,
    InvalidSegmentIndex(usize),
    MissingSampleCount,
    SampleRateMismatch { expected: u32, actual: u32 },
    SampleCountMismatch { expected: usize, actual: usize },
    IncompleteAssembly,
}

#[derive(Clone, Debug)]
pub enum RetryCommit {
    Replace { samples: Vec<f32>, sample_rate: u32 },
    KeepExisting,
}

#[derive(Clone, Debug)]
pub struct TtsAudioSegment {
    pub text: String,
    pub request_payload: String,
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub text_expanded: bool,
}

impl TtsAudioSegment {
    pub fn pending(text: impl Into<String>, request_payload: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            request_payload: request_payload.into(),
            samples: Arc::new(Vec::new()),
            sample_rate: 0,
            text_expanded: false,
        }
    }

    pub fn completed(
        text: impl Into<String>,
        request_payload: impl Into<String>,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Self {
        Self {
            text: text.into(),
            request_payload: request_payload.into(),
            samples: Arc::new(samples),
            sample_rate,
            text_expanded: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TtsAudioSegments {
    segments: Vec<TtsAudioSegment>,
    sample_rate: u32,
    start_offsets: Vec<usize>,
    revision: u64,
}

impl TtsAudioSegments {
    pub fn new(segments: Vec<TtsAudioSegment>) -> Result<Self, SegmentAudioError> {
        let Some(first) = segments.first() else {
            return Err(SegmentAudioError::EmptySegments);
        };
        let sample_rate = first.sample_rate;
        if let Some(segment) = segments
            .iter()
            .find(|segment| segment.sample_rate != sample_rate)
        {
            return Err(SegmentAudioError::SampleRateMismatch {
                expected: sample_rate,
                actual: segment.sample_rate,
            });
        }

        let start_offsets = Self::build_start_offsets(&segments);
        Ok(Self {
            segments,
            sample_rate,
            start_offsets,
            revision: next_audio_revision(),
        })
    }

    fn build_start_offsets(segments: &[TtsAudioSegment]) -> Vec<usize> {
        let mut offsets = Vec::with_capacity(segments.len() + 1);
        offsets.push(0usize);
        for segment in segments {
            let next = offsets
                .last()
                .copied()
                .unwrap_or_default()
                .saturating_add(segment.samples.len());
            offsets.push(next);
        }
        offsets
    }

    fn rebuild_offsets_and_revision(&mut self) {
        self.start_offsets = Self::build_start_offsets(&self.segments);
        self.revision = next_audio_revision();
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn total_samples(&self) -> usize {
        self.start_offsets.last().copied().unwrap_or_default()
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.total_samples() as f64 / self.sample_rate as f64
        }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn segment(&self, index: usize) -> Option<&TtsAudioSegment> {
        self.segments.get(index)
    }

    pub fn segment_mut(&mut self, index: usize) -> Option<&mut TtsAudioSegment> {
        self.segments.get_mut(index)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &TtsAudioSegment> {
        self.segments.iter()
    }

    pub fn merged_samples(&self) -> Vec<f32> {
        self.read_block(0, self.total_samples())
    }

    pub fn read_block(&self, start_sample: usize, max_samples: usize) -> Vec<f32> {
        if max_samples == 0 || start_sample >= self.total_samples() {
            return Vec::new();
        }

        let end_sample = start_sample
            .saturating_add(max_samples)
            .min(self.total_samples());
        let mut output = Vec::with_capacity(end_sample - start_sample);
        let mut segment_index = self
            .index_at_sample(start_sample)
            .unwrap_or(self.segments.len());
        let mut cursor = start_sample;

        while cursor < end_sample && segment_index < self.segments.len() {
            let segment_start = self.start_offsets[segment_index];
            let segment = &self.segments[segment_index];
            let local_start = cursor.saturating_sub(segment_start);
            let available = segment.samples.len().saturating_sub(local_start);
            let take = available.min(end_sample - cursor);
            output.extend_from_slice(&segment.samples[local_start..local_start + take]);
            cursor = cursor.saturating_add(take);
            segment_index += 1;
        }

        output
    }

    pub fn start_sample(&self, index: usize) -> Option<usize> {
        (index < self.segments.len()).then(|| self.start_offsets[index])
    }

    pub fn start_time_secs(&self, index: usize) -> Option<f64> {
        let start_sample = self.start_sample(index)?;
        (self.sample_rate != 0).then_some(start_sample as f64 / self.sample_rate as f64)
    }

    pub fn index_at_sample(&self, sample: usize) -> Option<usize> {
        if sample >= self.total_samples() {
            return None;
        }
        let boundary = self
            .start_offsets
            .partition_point(|offset| *offset <= sample);
        Some(boundary.saturating_sub(1).min(self.segments.len() - 1))
    }

    pub fn replace_samples(
        &mut self,
        index: usize,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Result<(), SegmentAudioError> {
        if sample_rate != self.sample_rate {
            return Err(SegmentAudioError::SampleRateMismatch {
                expected: self.sample_rate,
                actual: sample_rate,
            });
        }
        let Some(segment) = self.segments.get_mut(index) else {
            return Err(SegmentAudioError::InvalidSegmentIndex(index));
        };
        segment.samples = Arc::new(samples);
        segment.sample_rate = sample_rate;
        self.rebuild_offsets_and_revision();
        Ok(())
    }

    pub fn apply_retry(
        &mut self,
        index: usize,
        commit: RetryCommit,
    ) -> Result<bool, SegmentAudioError> {
        match commit {
            RetryCommit::Replace {
                samples,
                sample_rate,
            } => {
                self.replace_samples(index, samples, sample_rate)?;
                Ok(true)
            }
            RetryCommit::KeepExisting => Ok(false),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActiveSegmentAssembly {
    pub index: usize,
    samples: Vec<f32>,
    sample_rate: Option<u32>,
    expected_samples: Option<usize>,
}

impl ActiveSegmentAssembly {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            samples: Vec::new(),
            sample_rate: None,
            expected_samples: None,
        }
    }

    pub fn push_audio(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<(), SegmentAudioError> {
        if let Some(expected_rate) = self.sample_rate {
            if expected_rate != sample_rate {
                return Err(SegmentAudioError::SampleRateMismatch {
                    expected: expected_rate,
                    actual: sample_rate,
                });
            }
        } else {
            self.sample_rate = Some(sample_rate);
        }
        self.samples.extend_from_slice(samples);
        self.validate_sample_count()?;
        Ok(())
    }

    pub fn mark_complete(
        &mut self,
        sample_count: Option<usize>,
    ) -> Result<bool, SegmentAudioError> {
        self.expected_samples = Some(sample_count.ok_or(SegmentAudioError::MissingSampleCount)?);
        self.validate_sample_count()?;
        Ok(self.is_ready())
    }

    pub fn is_ready(&self) -> bool {
        self.expected_samples == Some(self.samples.len()) && self.sample_rate.is_some()
    }

    pub fn into_samples(self) -> Result<(Vec<f32>, u32), SegmentAudioError> {
        if !self.is_ready() {
            return Err(SegmentAudioError::IncompleteAssembly);
        }
        Ok((self.samples, self.sample_rate.unwrap_or_default()))
    }

    fn validate_sample_count(&self) -> Result<(), SegmentAudioError> {
        if let Some(expected) = self.expected_samples {
            if self.samples.len() > expected {
                return Err(SegmentAudioError::SampleCountMismatch {
                    expected,
                    actual: self.samples.len(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_completed_segments() -> TtsAudioSegments {
        TtsAudioSegments::new(vec![
            TtsAudioSegment::completed("第一段", "payload-1", vec![0.1, 0.2], 24_000),
            TtsAudioSegment::completed("第二段", "payload-2", vec![0.3, 0.4, 0.5], 24_000),
        ])
        .unwrap()
    }

    #[test]
    fn merged_segments_expose_contiguous_offsets_and_current_segment() {
        let segments = two_completed_segments();

        assert_eq!(segments.merged_samples(), vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(segments.start_sample(0), Some(0));
        assert_eq!(segments.start_sample(1), Some(2));
        assert_eq!(segments.index_at_sample(0), Some(0));
        assert_eq!(segments.index_at_sample(2), Some(1));
        assert_eq!(segments.index_at_sample(5), None);
    }

    #[test]
    fn replacement_changes_only_the_selected_segment() {
        let mut segments = two_completed_segments();

        segments.replace_samples(1, vec![0.9], 24_000).unwrap();

        assert_eq!(segments.segment(0).unwrap().samples.as_slice(), &[0.1, 0.2]);
        assert_eq!(segments.merged_samples(), vec![0.1, 0.2, 0.9]);
    }

    #[test]
    fn replacement_rejects_a_mismatched_sample_rate() {
        let mut segments = two_completed_segments();

        assert_eq!(
            segments.replace_samples(1, vec![0.9], 32_000),
            Err(SegmentAudioError::SampleRateMismatch {
                expected: 24_000,
                actual: 32_000,
            })
        );
    }

    #[test]
    fn assembly_waits_for_reported_samples_after_completion() {
        let mut assembly = ActiveSegmentAssembly::new(1);

        assert!(!assembly.mark_complete(Some(3)).unwrap());
        assembly.push_audio(&[0.1, 0.2], 24_000).unwrap();
        assert!(!assembly.is_ready());
        assembly.push_audio(&[0.3], 24_000).unwrap();

        assert!(assembly.is_ready());
        assert_eq!(
            assembly.into_samples().unwrap(),
            (vec![0.1, 0.2, 0.3], 24_000)
        );
    }

    #[test]
    fn retry_commit_replaces_only_the_target_and_exposes_its_start_time() {
        let mut segments = two_completed_segments();

        assert!(segments
            .apply_retry(
                1,
                RetryCommit::Replace {
                    samples: vec![0.9],
                    sample_rate: 24_000,
                },
            )
            .unwrap());

        assert_eq!(segments.segment(0).unwrap().samples.as_slice(), &[0.1, 0.2]);
        assert_eq!(segments.merged_samples(), vec![0.1, 0.2, 0.9]);
        assert_eq!(segments.start_time_secs(1), Some(2.0 / 24_000.0));
    }

    #[test]
    fn discarded_retry_keeps_the_existing_segment_audio() {
        let mut segments = two_completed_segments();
        let before = segments.merged_samples();

        assert!(!segments.apply_retry(1, RetryCommit::KeepExisting).unwrap());

        assert_eq!(segments.merged_samples(), before);
    }
}
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_AUDIO_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_audio_revision() -> u64 {
    NEXT_AUDIO_REVISION.fetch_add(1, Ordering::Relaxed)
}
