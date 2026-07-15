use moxin_voice::playback_time_stretch::{
    smooth_block_boundary, stretch_block_preserve_pitch, BlockTrim, StretchBlockCache,
    StretchBlockKey, StretchError,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SAMPLE_RATE: usize = 24_000;
const CONTEXT_SAMPLES: usize = SAMPLE_RATE / 5;
const RATES: [f64; 4] = [0.75, 1.25, 1.5, 2.0];
const RUNS: usize = 7;

fn speech_like_samples(sample_count: usize) -> Vec<f32> {
    let mut noise_state = 0x1234_5678u32;
    (0..sample_count)
        .map(|index| {
            noise_state = noise_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let noise = ((noise_state >> 8) as f32 / 0x00ff_ffff as f32 - 0.5) * 0.04;
            let time = index as f32 / SAMPLE_RATE as f32;
            let voiced = (std::f32::consts::TAU * 173.0 * time).sin() * 0.45
                + (std::f32::consts::TAU * 346.0 * time).sin() * 0.18
                + (std::f32::consts::TAU * 691.0 * time).sin() * 0.08;
            let envelope = 0.65 + 0.35 * (std::f32::consts::TAU * 3.7 * time).sin().abs();
            voiced * envelope + noise
        })
        .collect()
}

fn percentile_95(samples: &[Duration]) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len().saturating_sub(1));
    sorted[index]
}

fn benchmark_block(block_seconds: usize, rate: f64) -> Duration {
    let block_samples = block_seconds * SAMPLE_RATE;
    let input = speech_like_samples(block_samples + 2 * CONTEXT_SAMPLES);
    let trim = BlockTrim {
        leading_source_samples: CONTEXT_SAMPLES,
        output_source_samples: block_samples,
    };

    let _ = stretch_block_preserve_pitch(&input, rate, trim, &|| false).unwrap();
    let mut measurements = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let output = stretch_block_preserve_pitch(&input, rate, trim, &|| false).unwrap();
        measurements.push(started.elapsed());
        let expected = (block_samples as f64 / rate).round() as isize;
        assert!((output.len() as isize - expected).abs() < 4);
    }
    percentile_95(&measurements)
}

fn benchmark_cache_hit() -> Duration {
    let mut cache = StretchBlockCache::default();
    let key = StretchBlockKey::new(1, 0.75, 0);
    assert!(cache.insert(key, Arc::new(vec![0.0; 10 * SAMPLE_RATE])));
    let mut measurements = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        let started = Instant::now();
        assert!(cache.get(&key).is_some());
        measurements.push(started.elapsed());
    }
    percentile_95(&measurements)
}

fn benchmark_cached_block_queue_preparation() -> Duration {
    let mut cache = StretchBlockCache::default();
    let key = StretchBlockKey::new(1, 0.75, 0);
    assert!(cache.insert(key, Arc::new(vec![0.0; 320_000])));
    let mut measurements = Vec::with_capacity(100);
    for _ in 0..100 {
        let started = Instant::now();
        let cached = cache.get(&key).unwrap();
        let mut prepared = cached.as_ref().clone();
        smooth_block_boundary(&mut prepared, 0.25, SAMPLE_RATE / 50);
        let prepared = Arc::new(prepared);
        assert_eq!(prepared.len(), 320_000);
        measurements.push(started.elapsed());
    }
    percentile_95(&measurements)
}

fn benchmark_cancel_check() -> Duration {
    let input = speech_like_samples(10 * SAMPLE_RATE + 2 * CONTEXT_SAMPLES);
    let trim = BlockTrim {
        leading_source_samples: CONTEXT_SAMPLES,
        output_source_samples: 10 * SAMPLE_RATE,
    };
    let checks = AtomicUsize::new(0);
    let started = Instant::now();
    let result = stretch_block_preserve_pitch(&input, 0.75, trim, &|| {
        checks.fetch_add(1, Ordering::Relaxed) >= 1
    });
    assert_eq!(result, Err(StretchError::Cancelled));
    assert!(checks.load(Ordering::Relaxed) >= 2);
    started.elapsed()
}

fn main() {
    let mut ten_second_results = Vec::new();
    println!("moxin-voice time-stretch benchmark (24 kHz mono f32, p95 of {RUNS})");
    for block_seconds in [5, 10] {
        for rate in RATES {
            let p95 = benchmark_block(block_seconds, rate);
            println!(
                "block={block_seconds:>2}s rate={rate:>4.2}x p95={:>8.3} ms",
                p95.as_secs_f64() * 1_000.0
            );
            if block_seconds == 10 {
                ten_second_results.push((rate, p95));
            }
        }
    }

    let cache_hit = benchmark_cache_hit();
    let cached_block_ready = benchmark_cached_block_queue_preparation();
    let cancel = benchmark_cancel_check();
    println!(
        "cache-hit p95={:.6} ms; cached 0.75x block queue-prep p95={:.3} ms; cancellation-observed={:.3} ms",
        cache_hit.as_secs_f64() * 1_000.0,
        cached_block_ready.as_secs_f64() * 1_000.0,
        cancel.as_secs_f64() * 1_000.0
    );

    let slowest = ten_second_results
        .iter()
        .map(|(_, duration)| *duration)
        .max()
        .unwrap_or_default();
    let sequential_60m_compute = slowest.mul_f64(360.0);
    println!(
        "conservative 60-minute sequential compute projection: {:.2} s (360 blocks)",
        sequential_60m_compute.as_secs_f64()
    );
    println!(
        "bounded memory contract: 64 MiB cache + 30 s player queue + one 10.4 s worker input/output set"
    );

    let rate_075_p95 = ten_second_results
        .iter()
        .find_map(|(rate, duration)| ((*rate - 0.75).abs() < f64::EPSILON).then_some(*duration))
        .unwrap();
    assert!(
        rate_075_p95 <= Duration::from_millis(300),
        "10 s at 0.75x exceeded the 300 ms p95 release gate"
    );
    assert!(
        cached_block_ready <= Duration::from_millis(50),
        "cached block queue preparation exceeded the 50 ms release gate"
    );
    assert!(
        cancel <= Duration::from_millis(100),
        "cancellation check exceeded the 100 ms release gate"
    );
    assert!(
        slowest < Duration::from_secs(5),
        "block processing is too slow to prefetch ahead of 2x playback"
    );
}
