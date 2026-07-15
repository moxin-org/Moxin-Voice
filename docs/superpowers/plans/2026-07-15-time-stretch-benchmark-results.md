# Time-stretch release benchmark — 2026-07-15

## Reference environment

- Apple M4 Mac mini, 10 cores, 16 GB RAM
- macOS 26.5 (arm64)
- Rust/Cargo 1.96.0
- `moxin-voice` release profile
- 24 kHz mono `f32`, deterministic speech-like multi-tone/noise fixture
- p95 from 7 measured runs after one warm-up run

Reproduce with:

```bash
cargo run -p moxin-voice --example time_stretch_benchmark --release
```

The benchmark contains executable release-gate assertions. It exits non-zero if
0.75x processing exceeds 300 ms per 10-second block, cached block preparation
exceeds 50 ms, cancellation observation exceeds 100 ms, or the slowest block is
not fast enough to stay ahead of 2x playback.

## Results

| Source block | Rate | Processing p95 |
| --- | ---: | ---: |
| 5 s | 0.75x | 44.494 ms |
| 5 s | 1.25x | 26.902 ms |
| 5 s | 1.5x | 22.409 ms |
| 5 s | 2x | 16.829 ms |
| 10 s | 0.75x | 87.096 ms |
| 10 s | 1.25x | 52.734 ms |
| 10 s | 1.5x | 43.881 ms |
| 10 s | 2x | 32.529 ms |

Additional measurements:

- Cache lookup p95: 0.000042 ms.
- Cached 0.75x block lookup, clone, and 20 ms boundary smoothing p95: 0.017 ms.
- Cancellation observed at the next internal check: 0.027 ms.
- Conservative 60-minute sequential projection at the slowest measured p95:
  31.35 seconds for 360 ten-second blocks.

## Capacity and latency interpretation

The player does not process all 60 minutes before playback. An uncached current
block is prepared first, then three nearby blocks are prefetched. On this
reference machine, the measured 0.75x engine p95 is about 87 ms. With the UI's
100 ms polling interval, expected uncached first-block readiness is normally
within roughly 100–200 ms, below the 500 ms release gate. A 2x ten-second source
block yields five seconds of playable audio while its measured processing p95 is
about 33 ms, leaving substantial prefetch headroom.

The 60-minute source itself is allowed to occupy about 345.6 MB as the single
canonical PCM representation. The implementation never allocates the roughly
460.8 MB full 0.75x result. Additional playback memory stays bounded independent
of duration:

- stretch cache: 64 MiB hard budget;
- player circular queue: 30 seconds, about 2.75 MiB at 24 kHz mono `f32`;
- one 10-second block plus 200 ms context on each side and the 0.75x engine's
  output/weight/result working buffers: approximately 5 MiB;
- one temporary boundary-smoothed block: at most about 1.22 MiB at 0.75x.

This remains well below the 128 MiB non-canonical playback budget. Pending worker
requests share the canonical source and carry coordinates; they do not copy their
audio blocks on the UI thread.

## Engine decision

For this release, keep `LegacyWsolaEngine` behind the new `TimeStretchEngine`
interface. It clears the performance gates with large margin, is bounded to one
block, runs on one cancellable worker, and passes duration, pitch-period,
boundary-energy, revision, LRU, and latest-task-wins tests. No new third-party DSP
dependency is introduced without separate license, build, speech-quality, and
listening evaluation.

These synthetic measurements validate performance and basic signal invariants;
the release smoke test must still include real long-form speech listening for
metallic artifacts, repeated syllables, and boundary defects.

## Mature engine evaluation

The release does not add a DSP dependency merely because an engine has a suitable
API. The following candidates were checked against their official documentation:

| Candidate | Fit | Release decision |
| --- | --- | --- |
| [Signalsmith Stretch](https://github.com/Signalsmith-Audio/signalsmith-stretch) | MIT; C++11; block processing, seek/reset/flush, latency reporting and split-computation mode; primarily tested with AppleClang; an external Rust wrapper exists. | Best future evaluation candidate, but its own documentation says time stretching sounds best from 0.75x to 1.5x, while this product formally supports 2x. Do not ship it until the wrapper, Apple build, 2x speech quality, cancellation and the same benchmark matrix are verified. |
| [Rubber Band](https://breakfastquay.com/rubberband/) | High-quality C++ engine with streaming real-time mode and explicit real-time safety guidance. | Open-source distribution is GPL and proprietary distribution requires a commercial licence. Do not add it without a product licensing decision and an FFI/build evaluation. |
| [SoundTouch](https://www.surina.net/soundtouch/index.html) | C++; supports 32-bit float mono, macOS and real-time pitch-preserving tempo changes. | LGPL 2.1 or a separate commercial licence. It would add native build and redistribution obligations and still requires long-form speech listening; no benefit justifies that release risk after the current engine cleared the gates. |

Accordingly, none is eligible for this release yet. The stable
`TimeStretchEngine` boundary keeps a later replacement local to one module and
lets every candidate run against the existing benchmark and quality tests before
selection.
