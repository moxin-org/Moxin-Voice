use crate::tts_segments::TtsAudioSegments;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_SOURCE_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_source_revision() -> u64 {
    NEXT_SOURCE_REVISION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
enum PlaybackAudioStorage {
    Segments(TtsAudioSegments),
    Contiguous {
        samples: Arc<Vec<f32>>,
        source_sample_rate: u32,
    },
    #[cfg(test)]
    SyntheticSilence,
}

/// Immutable canonical audio with bounded, output-rate block reads.
///
/// Segment-backed sources share each generated segment. Contiguous sources keep
/// a single canonical buffer and resample from global sample coordinates, so a
/// block boundary does not reset interpolation phase.
#[derive(Clone, Debug)]
pub struct PlaybackAudioSource {
    storage: PlaybackAudioStorage,
    output_sample_rate: u32,
    total_output_samples: usize,
    revision: u64,
}

impl PlaybackAudioSource {
    pub fn from_segments(segments: TtsAudioSegments) -> Self {
        let output_sample_rate = segments.sample_rate();
        let total_output_samples = segments.total_samples();
        Self {
            storage: PlaybackAudioStorage::Segments(segments),
            output_sample_rate,
            total_output_samples,
            revision: next_source_revision(),
        }
    }

    pub fn from_contiguous(
        samples: Arc<Vec<f32>>,
        source_sample_rate: u32,
        output_sample_rate: u32,
    ) -> Self {
        let total_output_samples =
            scaled_sample_count(samples.len(), source_sample_rate, output_sample_rate);
        Self {
            storage: PlaybackAudioStorage::Contiguous {
                samples,
                source_sample_rate,
            },
            output_sample_rate,
            total_output_samples,
            revision: next_source_revision(),
        }
    }

    #[cfg(test)]
    fn synthetic_silence(total_output_samples: usize, output_sample_rate: u32) -> Self {
        Self {
            storage: PlaybackAudioStorage::SyntheticSilence,
            output_sample_rate,
            total_output_samples,
            revision: next_source_revision(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.output_sample_rate
    }

    pub fn total_samples(&self) -> usize {
        self.total_output_samples
    }

    pub fn is_empty(&self) -> bool {
        self.total_output_samples == 0
    }

    pub fn duration_secs(&self) -> f64 {
        if self.output_sample_rate == 0 {
            0.0
        } else {
            self.total_output_samples as f64 / self.output_sample_rate as f64
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn canonical_sample_bytes(&self) -> usize {
        match &self.storage {
            PlaybackAudioStorage::Segments(segments) => segments
                .iter()
                .map(|segment| segment.samples.len().saturating_mul(size_of::<f32>()))
                .sum(),
            PlaybackAudioStorage::Contiguous { samples, .. } => {
                samples.len().saturating_mul(size_of::<f32>())
            }
            #[cfg(test)]
            PlaybackAudioStorage::SyntheticSilence => 0,
        }
    }

    pub fn read_block(&self, start_sample: usize, max_samples: usize) -> Vec<f32> {
        if max_samples == 0 || start_sample >= self.total_output_samples {
            return Vec::new();
        }
        let requested = max_samples.min(self.total_output_samples - start_sample);

        match &self.storage {
            PlaybackAudioStorage::Segments(segments) => {
                segments.read_block(start_sample, requested)
            }
            PlaybackAudioStorage::Contiguous {
                samples,
                source_sample_rate,
            } => read_resampled_block(
                samples,
                *source_sample_rate,
                self.output_sample_rate,
                start_sample,
                requested,
            ),
            #[cfg(test)]
            PlaybackAudioStorage::SyntheticSilence => vec![0.0; requested],
        }
    }

    pub fn block_index_at_time(&self, time_secs: f64, block_samples: usize) -> u64 {
        if block_samples == 0 || self.output_sample_rate == 0 {
            return 0;
        }
        let sample = (time_secs.max(0.0) * self.output_sample_rate as f64).floor() as usize;
        (sample.min(self.total_output_samples) / block_samples) as u64
    }

    pub fn block_time_range(&self, block_index: u64, block_samples: usize) -> (f64, f64) {
        if block_samples == 0 || self.output_sample_rate == 0 {
            return (0.0, 0.0);
        }
        let start = (block_index as usize)
            .saturating_mul(block_samples)
            .min(self.total_output_samples);
        let end = start
            .saturating_add(block_samples)
            .min(self.total_output_samples);
        (
            start as f64 / self.output_sample_rate as f64,
            end as f64 / self.output_sample_rate as f64,
        )
    }
}

fn scaled_sample_count(input_len: usize, input_rate: u32, output_rate: u32) -> usize {
    if input_len == 0 || input_rate == 0 || output_rate == 0 {
        return 0;
    }
    if input_rate == output_rate {
        return input_len;
    }
    ((input_len as f64 * output_rate as f64 / input_rate as f64).round()) as usize
}

fn read_resampled_block(
    samples: &[f32],
    input_rate: u32,
    output_rate: u32,
    output_start: usize,
    output_len: usize,
) -> Vec<f32> {
    if output_len == 0 || samples.is_empty() || input_rate == 0 || output_rate == 0 {
        return Vec::new();
    }
    if input_rate == output_rate {
        let end = output_start.saturating_add(output_len).min(samples.len());
        return samples.get(output_start..end).unwrap_or_default().to_vec();
    }

    let source_per_output = input_rate as f64 / output_rate as f64;
    let mut output = Vec::with_capacity(output_len);
    for output_index in output_start..output_start.saturating_add(output_len) {
        let source_pos = output_index as f64 * source_per_output;
        let source_index = source_pos.floor() as usize;
        let fraction = (source_pos - source_index as f64) as f32;
        let first = samples.get(source_index).copied().unwrap_or(0.0);
        let second = samples.get(source_index + 1).copied().unwrap_or(first);
        output.push(first + (second - first) * fraction);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts_segments::TtsAudioSegment;

    #[test]
    fn segment_source_reads_across_boundaries_without_merging() {
        let segments = TtsAudioSegments::new(vec![
            TtsAudioSegment::completed("one", "p1", vec![1.0, 2.0], 24_000),
            TtsAudioSegment::completed("two", "p2", vec![3.0, 4.0, 5.0], 24_000),
        ])
        .unwrap();
        let source = PlaybackAudioSource::from_segments(segments);

        assert_eq!(source.read_block(1, 3), vec![2.0, 3.0, 4.0]);
        assert_eq!(source.total_samples(), 5);
        assert_eq!(source.canonical_sample_bytes(), 5 * size_of::<f32>());
    }

    #[test]
    fn contiguous_source_resamples_blocks_from_global_coordinates() {
        let source = PlaybackAudioSource::from_contiguous(Arc::new(vec![0.0, 1.0, 2.0, 3.0]), 4, 8);

        assert_eq!(source.total_samples(), 8);
        assert_eq!(source.read_block(1, 3), vec![0.5, 1.0, 1.5]);
        assert_eq!(source.read_block(4, 2), vec![2.0, 2.5]);
    }

    #[test]
    fn sixty_minute_source_allocates_only_the_requested_block() {
        let total = 60 * 60 * 24_000;
        let block = 10 * 24_000;
        let source = PlaybackAudioSource::synthetic_silence(total, 24_000);

        let samples = source.read_block(55 * 60 * 24_000, block);

        assert_eq!(source.duration_secs(), 60.0 * 60.0);
        assert_eq!(samples.len(), block);
        assert_eq!(samples.capacity(), block);
        assert_eq!(source.canonical_sample_bytes(), 0);
    }

    #[test]
    fn segment_retry_changes_revision_and_only_replaces_target_audio() {
        let mut segments = TtsAudioSegments::new(vec![
            TtsAudioSegment::completed("one", "p1", vec![1.0, 2.0], 24_000),
            TtsAudioSegment::completed("two", "p2", vec![3.0], 24_000),
        ])
        .unwrap();
        let old_source = PlaybackAudioSource::from_segments(segments.clone());

        segments.replace_samples(1, vec![9.0, 8.0], 24_000).unwrap();
        let source = PlaybackAudioSource::from_segments(segments);

        assert_ne!(source.revision(), old_source.revision());
        assert_eq!(source.read_block(0, 4), vec![1.0, 2.0, 9.0, 8.0]);
    }
}
