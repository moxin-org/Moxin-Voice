use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub const DEFAULT_STRETCH_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StretchBlockKey {
    pub audio_revision: u64,
    pub rate_milli: u16,
    pub block_index: u64,
}

impl StretchBlockKey {
    pub fn new(audio_revision: u64, playback_rate: f64, block_index: u64) -> Self {
        Self {
            audio_revision,
            rate_milli: rate_to_milli(playback_rate),
            block_index,
        }
    }

    pub fn playback_rate(self) -> f64 {
        self.rate_milli as f64 / 1000.0
    }
}

pub fn rate_to_milli(playback_rate: f64) -> u16 {
    (playback_rate.clamp(0.5, 2.0) * 1000.0).round() as u16
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlockTrim {
    pub leading_source_samples: usize,
    pub output_source_samples: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StretchError {
    Cancelled,
    InvalidRate,
}

pub trait TimeStretchEngine: Send + 'static {
    fn process_block(
        &mut self,
        input_with_context: &[f32],
        playback_rate: f64,
        trim: BlockTrim,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, StretchError>;
}

#[derive(Default)]
pub struct LegacyWsolaEngine;

impl TimeStretchEngine for LegacyWsolaEngine {
    fn process_block(
        &mut self,
        input_with_context: &[f32],
        playback_rate: f64,
        trim: BlockTrim,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, StretchError> {
        stretch_block_preserve_pitch(input_with_context, playback_rate, trim, is_cancelled)
    }
}

pub fn stretch_block_preserve_pitch(
    samples: &[f32],
    playback_rate: f64,
    trim: BlockTrim,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<f32>, StretchError> {
    if is_cancelled() {
        return Err(StretchError::Cancelled);
    }
    if !playback_rate.is_finite() || playback_rate <= 0.0 {
        return Err(StretchError::InvalidRate);
    }
    if samples.is_empty() || trim.output_source_samples == 0 {
        return Ok(Vec::new());
    }

    let rate = playback_rate.clamp(0.5, 2.0);
    if (rate - 1.0).abs() < 0.01 {
        let start = trim.leading_source_samples.min(samples.len());
        let end = start
            .saturating_add(trim.output_source_samples)
            .min(samples.len());
        return Ok(samples[start..end].to_vec());
    }

    let full_target_len = (samples.len() as f64 / rate).round().max(1.0) as usize;
    let window_len = samples.len().min(1024);
    if window_len < 32 {
        let start = trim.leading_source_samples.min(samples.len());
        let end = start
            .saturating_add(trim.output_source_samples)
            .min(samples.len());
        return Ok(samples[start..end].to_vec());
    }

    let synthesis_hop = (window_len / 4).max(8);
    let analysis_hop = (synthesis_hop as f64 * rate).round().max(1.0);
    let mut output = vec![0.0f32; full_target_len + window_len];
    let mut weights = vec![0.0f32; full_target_len + window_len];
    let mut source_pos = 0.0f64;
    let mut output_pos = 0usize;

    while output_pos < full_target_len {
        if is_cancelled() {
            return Err(StretchError::Cancelled);
        }
        let desired_source_idx = source_pos.round().max(0.0) as usize;
        let source_idx = best_time_stretch_source_index(
            samples,
            &output,
            &weights,
            output_pos,
            desired_source_idx,
            window_len,
            synthesis_hop,
            is_cancelled,
        )?;
        if source_idx >= samples.len() {
            break;
        }

        let frame_len = window_len
            .min(samples.len() - source_idx)
            .min(output.len() - output_pos);
        for i in 0..frame_len {
            let window = hann_window_sample(i, window_len).max(0.000_001);
            output[output_pos + i] += samples[source_idx + i] * window;
            weights[output_pos + i] += window;
        }

        source_pos += analysis_hop;
        output_pos = output_pos.saturating_add(synthesis_hop);
    }

    for i in 0..full_target_len {
        if weights[i] > 0.000_001 {
            output[i] /= weights[i];
        }
    }

    let trim_start = (trim.leading_source_samples as f64 / rate).round() as usize;
    let trimmed_len = (trim.output_source_samples as f64 / rate).round().max(1.0) as usize;
    let trim_end = trim_start.saturating_add(trimmed_len).min(full_target_len);
    if trim_start >= trim_end || trim_start >= output.len() {
        return Ok(Vec::new());
    }
    Ok(output[trim_start..trim_end].to_vec())
}

fn hann_window_sample(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = std::f32::consts::TAU * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}

#[allow(clippy::too_many_arguments)]
fn best_time_stretch_source_index(
    samples: &[f32],
    output: &[f32],
    weights: &[f32],
    output_pos: usize,
    desired_source_idx: usize,
    frame_len: usize,
    synthesis_hop: usize,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<usize, StretchError> {
    if output_pos == 0 || frame_len == 0 || samples.len() <= frame_len {
        return Ok(desired_source_idx.min(samples.len().saturating_sub(frame_len)));
    }

    let max_candidate = samples.len().saturating_sub(frame_len);
    let search_radius = (frame_len / 3).clamp(64, 320);
    let start = desired_source_idx
        .saturating_sub(search_radius)
        .min(max_candidate);
    let end = desired_source_idx
        .saturating_add(search_radius)
        .min(max_candidate);
    let compare_len = (frame_len.saturating_sub(synthesis_hop))
        .min(512)
        .min(output.len().saturating_sub(output_pos));
    if compare_len < 32 || start >= end {
        return Ok(desired_source_idx.min(max_candidate));
    }

    let mut best_idx = desired_source_idx.min(max_candidate);
    let mut best_score = f32::NEG_INFINITY;
    for candidate in start..=end {
        if candidate % 32 == 0 && is_cancelled() {
            return Err(StretchError::Cancelled);
        }
        let mut dot = 0.0f32;
        let mut out_energy = 0.0f32;
        let mut src_energy = 0.0f32;

        for offset in (0..compare_len).step_by(4) {
            if weights[output_pos + offset] <= 0.000_001 {
                continue;
            }
            let out_sample = output[output_pos + offset] / weights[output_pos + offset];
            let src_sample = samples[candidate + offset];
            dot += out_sample * src_sample;
            out_energy += out_sample * out_sample;
            src_energy += src_sample * src_sample;
        }

        if out_energy <= 0.000_001 || src_energy <= 0.000_001 {
            continue;
        }

        let score = dot / (out_energy.sqrt() * src_energy.sqrt());
        if score > best_score {
            best_score = score;
            best_idx = candidate;
        }
    }

    Ok(best_idx)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StretchPriority {
    Current,
    Prefetch,
}

struct StretchRequest {
    task_id: u64,
    key: StretchBlockKey,
    input_with_context: Vec<f32>,
    trim: BlockTrim,
    priority: StretchPriority,
}

enum WorkerCommand {
    Process(StretchRequest),
    Stop,
}

#[derive(Debug)]
pub struct StretchBlockResult {
    pub task_id: u64,
    pub key: StretchBlockKey,
    pub samples: Vec<f32>,
}

pub struct PlaybackStretchWorker {
    command_tx: Sender<WorkerCommand>,
    result_rx: Receiver<StretchBlockResult>,
    latest_task_id: Arc<AtomicU64>,
    next_task_id: u64,
    stopped: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl PlaybackStretchWorker {
    pub fn new() -> Self {
        Self::with_engine(LegacyWsolaEngine)
    }

    pub fn with_engine<E: TimeStretchEngine>(mut engine: E) -> Self {
        let (command_tx, command_rx) = unbounded::<WorkerCommand>();
        let (result_tx, result_rx) = unbounded::<StretchBlockResult>();
        let latest_task_id = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_latest_task_id = Arc::clone(&latest_task_id);
        let worker_stopped = Arc::clone(&stopped);
        let join_handle = std::thread::Builder::new()
            .name("moxin-playback-stretch".to_string())
            .spawn(move || {
                while !worker_stopped.load(Ordering::Acquire) {
                    let first = match command_rx.recv() {
                        Ok(WorkerCommand::Process(request)) => request,
                        Ok(WorkerCommand::Stop) | Err(_) => break,
                    };
                    let mut requests = vec![first];
                    let mut should_stop = false;
                    loop {
                        match command_rx.try_recv() {
                            Ok(WorkerCommand::Process(request)) => requests.push(request),
                            Ok(WorkerCommand::Stop) => {
                                should_stop = true;
                                break;
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                should_stop = true;
                                break;
                            }
                        }
                    }
                    if should_stop {
                        break;
                    }

                    requests.sort_by_key(|request| request.priority);
                    for request in requests {
                        if worker_stopped.load(Ordering::Acquire)
                            || request.task_id != worker_latest_task_id.load(Ordering::Acquire)
                        {
                            continue;
                        }
                        let cancelled = || {
                            worker_stopped.load(Ordering::Acquire)
                                || request.task_id != worker_latest_task_id.load(Ordering::Acquire)
                        };
                        let result = engine.process_block(
                            &request.input_with_context,
                            request.key.playback_rate(),
                            request.trim,
                            &cancelled,
                        );
                        if let Ok(samples) = result {
                            if !cancelled() {
                                let _ = result_tx.send(StretchBlockResult {
                                    task_id: request.task_id,
                                    key: request.key,
                                    samples,
                                });
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn playback stretch worker");

        Self {
            command_tx,
            result_rx,
            latest_task_id,
            next_task_id: 0,
            stopped,
            join_handle: Some(join_handle),
        }
    }

    pub fn begin_task(&mut self) -> u64 {
        self.next_task_id = self.next_task_id.wrapping_add(1).max(1);
        self.latest_task_id
            .store(self.next_task_id, Ordering::Release);
        self.next_task_id
    }

    pub fn submit(
        &self,
        task_id: u64,
        key: StretchBlockKey,
        input_with_context: Vec<f32>,
        trim: BlockTrim,
        priority: StretchPriority,
    ) -> bool {
        if task_id != self.latest_task_id.load(Ordering::Acquire) {
            return false;
        }
        self.command_tx
            .send(WorkerCommand::Process(StretchRequest {
                task_id,
                key,
                input_with_context,
                trim,
                priority,
            }))
            .is_ok()
    }

    pub fn try_recv(&self) -> Option<StretchBlockResult> {
        self.result_rx.try_recv().ok()
    }
}

impl Default for PlaybackStretchWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PlaybackStretchWorker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.latest_task_id.fetch_add(1, Ordering::AcqRel);
        let _ = self.command_tx.send(WorkerCommand::Stop);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

struct CacheEntry {
    samples: Arc<Vec<f32>>,
    bytes: usize,
    last_used: u64,
}

pub struct StretchBlockCache {
    entries: HashMap<StretchBlockKey, CacheEntry>,
    byte_budget: usize,
    used_bytes: usize,
    clock: u64,
}

impl StretchBlockCache {
    pub fn new(byte_budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            byte_budget,
            used_bytes: 0,
            clock: 0,
        }
    }

    pub fn get(&mut self, key: &StretchBlockKey) -> Option<Arc<Vec<f32>>> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.clock;
        Some(Arc::clone(&entry.samples))
    }

    pub fn contains(&self, key: &StretchBlockKey) -> bool {
        self.entries.contains_key(key)
    }

    pub fn insert(&mut self, key: StretchBlockKey, samples: Arc<Vec<f32>>) -> bool {
        let bytes = samples.len().saturating_mul(size_of::<f32>());
        if bytes > self.byte_budget {
            return false;
        }
        self.clock = self.clock.wrapping_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(previous.bytes);
        }
        while self.used_bytes.saturating_add(bytes) > self.byte_budget {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest_key) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.bytes);
            }
        }
        self.used_bytes = self.used_bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CacheEntry {
                samples,
                bytes,
                last_used: self.clock,
            },
        );
        true
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn byte_budget(&self) -> usize {
        self.byte_budget
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for StretchBlockCache {
    fn default() -> Self {
        Self::new(DEFAULT_STRETCH_CACHE_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct SlowCancellableEngine;

    impl TimeStretchEngine for SlowCancellableEngine {
        fn process_block(
            &mut self,
            input_with_context: &[f32],
            _playback_rate: f64,
            _trim: BlockTrim,
            is_cancelled: &dyn Fn() -> bool,
        ) -> Result<Vec<f32>, StretchError> {
            for _ in 0..100 {
                if is_cancelled() {
                    return Err(StretchError::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(input_with_context.to_vec())
        }
    }

    fn sine(sample_rate: usize, seconds: f64, frequency: f64) -> Vec<f32> {
        let len = (sample_rate as f64 * seconds) as usize;
        (0..len)
            .map(|index| {
                (std::f64::consts::TAU * frequency * index as f64 / sample_rate as f64).sin() as f32
            })
            .collect()
    }

    fn positive_zero_crossing_period(samples: &[f32]) -> f64 {
        let crossings: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter_map(|(index, pair)| (pair[0] <= 0.0 && pair[1] > 0.0).then_some(index))
            .collect();
        crossings
            .windows(2)
            .map(|pair| (pair[1] - pair[0]) as f64)
            .sum::<f64>()
            / crossings.len().saturating_sub(1).max(1) as f64
    }

    #[test]
    fn block_stretch_changes_duration_without_changing_main_period() {
        let input = sine(24_000, 1.0, 240.0);
        let trim = BlockTrim {
            leading_source_samples: 0,
            output_source_samples: input.len(),
        };

        let output = stretch_block_preserve_pitch(&input, 1.5, trim, &|| false).unwrap();

        assert!((output.len() as isize - 16_000).abs() < 4);
        let input_period = positive_zero_crossing_period(&input);
        let output_period = positive_zero_crossing_period(&output[512..output.len() - 512]);
        assert!((input_period - output_period).abs() < 2.0);
    }

    #[test]
    fn context_is_removed_from_block_output() {
        let input = sine(24_000, 1.4, 220.0);
        let trim = BlockTrim {
            leading_source_samples: 4_800,
            output_source_samples: 24_000,
        };

        let output = stretch_block_preserve_pitch(&input, 0.75, trim, &|| false).unwrap();

        assert!((output.len() as isize - 32_000).abs() < 4);
    }

    #[test]
    fn cancellation_is_checked_inside_processing() {
        let input = sine(24_000, 2.0, 220.0);
        let trim = BlockTrim {
            leading_source_samples: 0,
            output_source_samples: input.len(),
        };

        let result = stretch_block_preserve_pitch(&input, 0.75, trim, &|| true);

        assert_eq!(result, Err(StretchError::Cancelled));
    }

    #[test]
    fn worker_only_publishes_results_for_the_latest_task() {
        let mut worker = PlaybackStretchWorker::with_engine(SlowCancellableEngine);
        let old_task = worker.begin_task();
        let old_key = StretchBlockKey::new(1, 0.75, 0);
        assert!(worker.submit(
            old_task,
            old_key,
            vec![1.0],
            BlockTrim {
                leading_source_samples: 0,
                output_source_samples: 1,
            },
            StretchPriority::Current,
        ));

        std::thread::sleep(Duration::from_millis(10));
        let latest_task = worker.begin_task();
        let latest_key = StretchBlockKey::new(1, 1.25, 0);
        assert!(worker.submit(
            latest_task,
            latest_key,
            vec![2.0],
            BlockTrim {
                leading_source_samples: 0,
                output_source_samples: 1,
            },
            StretchPriority::Current,
        ));

        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            if let Some(result) = worker.try_recv() {
                break result;
            }
            assert!(Instant::now() < deadline, "latest stretch task timed out");
            std::thread::sleep(Duration::from_millis(5));
        };

        assert_eq!(result.task_id, latest_task);
        assert_eq!(result.key, latest_key);
        assert_eq!(result.samples, vec![2.0]);
        assert!(worker.try_recv().is_none());
    }

    #[test]
    fn cache_is_revision_scoped_and_evicts_to_its_byte_budget() {
        let mut cache = StretchBlockCache::new(32);
        let first = StretchBlockKey::new(1, 1.5, 0);
        let second = StretchBlockKey::new(1, 1.5, 1);
        let other_revision = StretchBlockKey::new(2, 1.5, 0);

        assert!(cache.insert(first, Arc::new(vec![1.0; 4])));
        assert!(cache.insert(second, Arc::new(vec![2.0; 4])));
        assert!(cache.contains(&first));
        assert!(cache.contains(&second));
        assert!(cache.insert(other_revision, Arc::new(vec![3.0; 4])));

        assert!(!cache.contains(&first));
        assert!(cache.contains(&second));
        assert!(cache.contains(&other_revision));
        assert!(cache.used_bytes() <= cache.byte_budget());
    }

    #[test]
    fn cache_rejects_a_single_oversized_block() {
        let mut cache = StretchBlockCache::new(8);
        let key = StretchBlockKey::new(1, 0.75, 0);

        assert!(!cache.insert(key, Arc::new(vec![0.0; 3])));
        assert!(cache.is_empty());
    }

    #[test]
    fn rate_keys_are_normalized_to_integer_milli_units() {
        assert_eq!(rate_to_milli(0.75), 750);
        assert_eq!(rate_to_milli(1.25), 1_250);
        assert_eq!(StretchBlockKey::new(7, 1.5, 2).rate_milli, 1_500);
    }
}
