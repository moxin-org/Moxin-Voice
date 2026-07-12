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
    Replace {
        samples: Vec<f32>,
        sample_rate: u32,
    },
    KeepExisting,
}

#[derive(Clone, Debug)]
pub struct TtsAudioSegment {
    pub text: String,
    pub request_payload: String,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub text_expanded: bool,
}

impl TtsAudioSegment {
    pub fn pending(text: impl Into<String>, request_payload: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            request_payload: request_payload.into(),
            samples: Vec::new(),
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
            samples,
            sample_rate,
            text_expanded: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TtsAudioSegments {
    segments: Vec<TtsAudioSegment>,
    sample_rate: u32,
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

        Ok(Self {
            segments,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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
        self.segments
            .iter()
            .flat_map(|segment| segment.samples.iter().copied())
            .collect()
    }

    pub fn start_sample(&self, index: usize) -> Option<usize> {
        if index >= self.segments.len() {
            return None;
        }
        Some(
            self.segments[..index]
                .iter()
                .map(|segment| segment.samples.len())
                .sum(),
        )
    }

    pub fn start_time_secs(&self, index: usize) -> Option<f64> {
        let start_sample = self.start_sample(index)?;
        (self.sample_rate != 0).then_some(start_sample as f64 / self.sample_rate as f64)
    }

    pub fn index_at_sample(&self, sample: usize) -> Option<usize> {
        let mut start = 0usize;
        for (index, segment) in self.segments.iter().enumerate() {
            let end = start.saturating_add(segment.samples.len());
            if sample >= start && sample < end {
                return Some(index);
            }
            start = end;
        }
        None
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
        segment.samples = samples;
        segment.sample_rate = sample_rate;
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

        assert_eq!(segments.segment(0).unwrap().samples, vec![0.1, 0.2]);
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
        assert_eq!(assembly.into_samples().unwrap(), (vec![0.1, 0.2, 0.3], 24_000));
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

        assert_eq!(segments.segment(0).unwrap().samples, vec![0.1, 0.2]);
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
