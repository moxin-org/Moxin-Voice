//! Lightweight audio post-processing for runtime controls.
//!
//! - `speed`: pitch-preserving time-stretch via WSOLA
//! - `volume`: simple gain
//! - `pitch`: semitone shift with duration preservation
//!   via resample + WSOLA time-stretch (higher quality, pure Rust)

use crate::TtsParams;

const EPS: f32 = 1e-4;

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

fn hann_window(n: usize) -> Vec<f32> {
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![1.0];
    }
    let denom = (n - 1) as f32;
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / denom;
            0.5 - 0.5 * x.cos()
        })
        .collect()
}

fn resample_linear(input: &[f32], out_len: usize) -> Vec<f32> {
    if input.is_empty() || out_len == 0 {
        return Vec::new();
    }
    if input.len() == 1 {
        return vec![input[0]; out_len];
    }
    if out_len == 1 {
        return vec![input[0]];
    }

    let in_last = (input.len() - 1) as f32;
    let out_last = (out_len - 1) as f32;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = (i as f32) * in_last / out_last;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f32;
        let a = input[idx];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn normalized_corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return -1.0;
    }
    let mut dot = 0.0f32;
    let mut ea = 0.0f32;
    let mut eb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        ea += a[i] * a[i];
        eb += b[i] * b[i];
    }
    if ea <= EPS || eb <= EPS {
        return -1.0;
    }
    dot / (ea.sqrt() * eb.sqrt() + EPS)
}

fn time_stretch_wsola(input: &[f32], out_len: usize) -> Vec<f32> {
    if input.is_empty() || out_len == 0 {
        return Vec::new();
    }
    if input.len() < 2048 || out_len < 2048 {
        return resample_linear(input, out_len);
    }

    let frame = 1024usize;
    let synthesis_hop = 256usize;
    let overlap = frame - synthesis_hop;
    let search = 128isize;

    let stretch = out_len as f32 / input.len() as f32;
    // For WSOLA: choose analysis hop from target stretch ratio.
    let analysis_hop_nominal = ((synthesis_hop as f32) / stretch).max(1.0);
    let win = hann_window(frame);

    let mut out = vec![0.0f32; out_len + frame + 4];
    let mut norm = vec![0.0f32; out_len + frame + 4];

    // Seed first frame at origin.
    for j in 0..frame.min(input.len()).min(out.len()) {
        let w = win[j];
        out[j] += input[j] * w;
        norm[j] += w * w;
    }

    let mut prev_in = 0usize;
    let mut prev_out = 0usize;

    loop {
        let out_pos = prev_out + synthesis_hop;
        if out_pos >= out_len {
            break;
        }

        let expected = prev_in as f32 + analysis_hop_nominal;
        let mut best_in = expected.round().max(0.0) as isize;
        let mut best_score = -1.0f32;

        // Find the analysis frame that best matches current output overlap.
        let ref_start = out_pos;
        let ref_end = (ref_start + overlap).min(out.len());
        if ref_end <= ref_start + 16 {
            break;
        }
        let out_ref = &out[ref_start..ref_end];

        let center = expected.round() as isize;
        for delta in -search..=search {
            let cand = center + delta;
            if cand < 0 {
                continue;
            }
            let cand_u = cand as usize;
            let cand_end = cand_u + overlap;
            if cand_end >= input.len() {
                continue;
            }
            let input_ref = &input[cand_u..cand_end];
            let score = normalized_corr(out_ref, input_ref);
            if score > best_score {
                best_score = score;
                best_in = cand;
            }
        }

        let in_pos = best_in.max(0) as usize;
        if in_pos + 32 >= input.len() {
            break;
        }

        for j in 0..frame {
            let i_idx = in_pos + j;
            let o_idx = out_pos + j;
            if i_idx >= input.len() || o_idx >= out.len() {
                continue;
            }

            let w = win[j];
            out[o_idx] += input[i_idx] * w;
            norm[o_idx] += w * w;
        }

        prev_in = in_pos;
        prev_out = out_pos;
    }

    for (y, n) in out.iter_mut().zip(norm.iter()) {
        if *n > EPS {
            *y /= *n;
        }
    }
    out.truncate(out_len);
    out
}

fn apply_pitch_with_duration_lock(samples: &[f32], semitones: f32) -> Vec<f32> {
    let semitones = clamp(semitones, -12.0, 12.0);
    if semitones.abs() < EPS || samples.len() < 256 {
        return samples.to_vec();
    }

    // factor > 1.0 => pitch up
    let factor = 2.0f32.powf(semitones / 12.0);
    let tmp_len = ((samples.len() as f32) / factor).round().max(1.0) as usize;
    let pitched = resample_linear(samples, tmp_len);
    time_stretch_wsola(&pitched, samples.len())
}

fn apply_speed_with_pitch_lock(samples: &[f32], speed: f32) -> Vec<f32> {
    let speed = clamp(speed, 0.5, 2.0);
    if (speed - 1.0).abs() < EPS || samples.len() < 256 {
        return samples.to_vec();
    }

    let out_len = ((samples.len() as f32) / speed).round().max(1.0) as usize;
    time_stretch_wsola(samples, out_len)
}

fn apply_volume(samples: &mut [f32], volume: f32) {
    let gain = clamp(volume, 0.0, 200.0) / 100.0;
    if (gain - 1.0).abs() < EPS {
        return;
    }
    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

pub fn apply_runtime_audio_params(samples: Vec<f32>, params: &TtsParams) -> Vec<f32> {
    if samples.is_empty() {
        return samples;
    }

    let mut out = if let Some(speed) = params.speed {
        apply_speed_with_pitch_lock(&samples, speed)
    } else {
        samples
    };

    out = if let Some(pitch) = params.pitch {
        apply_pitch_with_duration_lock(&out, pitch)
    } else {
        out
    };

    if let Some(volume) = params.volume {
        apply_volume(&mut out, volume);
    }

    out
}
