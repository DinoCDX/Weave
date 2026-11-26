# Benchmark log

Every benchmark run for this project is recorded here, with the command, the environment, the raw result, and what it does and does not show. A change that claims a speed effect must add an entry first. Entries are never edited after the fact. A later run goes in as a new entry, and it names the entry it replaces.

## Environment for all entries below

- Host: virtual machine (Firecracker) on a shared host, 1 vCPU, Intel Xeon @ 2.30 GHz, 4 GB RAM, Linux 6.18.
- Toolchain: rustc and cargo 1.89.0 (the pinned version), wgpu 30.0.2, glam 0.34.3, hecs 0.11.2.
- GPU: none. Vulkan runs on Mesa's software driver (lavapipe) on Xvfb. The renderer numbers are CPU wall time of each pass, not GPU time.
- The host is shared, so identical runs vary. Single runs are not reliable here (see B-000).

## How to run

```
cargo run --release -p heart-bench                      # B-001 heart tick cost
target/release/render-bench --scenario dense --count 10000 --mode batched --cull on --sync on --frames 200   # B-004
target/release/render-bench --scenario reload                                                              # B-005
cargo build --release -p cube                           # build the renderer example
xvfb-run -a -s "-screen 0 800x600x24" sh run.sh         # run cube for N frames (see B-002)
```

`run.sh` for B-002 sets `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` and runs `target/release/cube --frames 600`.

## B-001: heart tick cost (release, best of seven)

Source: `examples/heart-bench`. Each case builds a scene, ticks 200 frames seven times, and reports the fastest of the seven as µs per frame and ns per object per frame.

Run on 2026-11-26, after the services change (decision D-74). Code state: `WeaveEngineX.zip` from the services change.

| case | objects | scripted | µs/frame | ns/object |
|---|---:|---:|---:|---:|
| plain objects | 10,000 | 0 | 89.6 | 9.0 |
| half scripted | 10,000 | 5,000 | 187.8 | 18.8 |
| all scripted | 10,000 | 10,000 | 499.8 | 50.0 |
| plain objects, large | 100,000 | 0 | 1327.1 | 13.3 |

Spread across identical runs of the same build (three more runs of all-scripted, best of seven each): 36 to 64 ns per object. The plain-object rows held within 9.0 to 9.5 ns across runs. The spread on the scripted rows is large enough that the services change (D-74) is not measurably different from the one before it. No speed claim is made for it.

## B-002: renderer pass timing, cube example, debug build

Command: `target/debug/cube --frames 300`, under Xvfb with lavapipe. Run 2026-11-26.

| pass | last (µs) | average (µs) | calls |
|---|---:|---:|---:|
| clear | 30.4 | 31.2 | 300 |
| opaque | 12.5 | 13.2 | 300 |
| transparent | 0.1 | 0.1 | 300 |
| post | 0.1 | 0.2 | 300 |

Debug build, so the numbers are not representative of release performance. Recorded as the first renderer measurement. Superseded by B-003 for comparison.

## B-003: renderer pass timing, cube example, release build

Command: `target/release/cube --frames 600`, under Xvfb with lavapipe. Run 2026-11-26, after B-001 on the same machine and build.

| pass | last (µs) | average (µs) | calls |
|---|---:|---:|---:|
| clear | 6.6 | 5.0 | 600 |
| opaque | 2.9 | 2.6 | 600 |
| transparent | 0.1 | 0.1 | 600 |
| post | 0.0 | 0.0 | 600 |

These are CPU times for recording and submitting each pass. Lavapipe executes the GPU work on the CPU, so the GPU cost is inside the frame and not in these numbers. They show how much CPU the graph itself spends per pass, which is what E3 will watch. Real GPU timings need hardware and the timestamp-query path (planned with E3.x).

## B-004: dense scene, batching and culling (release, GPU-synchronised)

Command: `target/release/render-bench --scenario dense --count 10000 --mode <batched|unbatched> --cull <on|off> --sync on --frames 200`, under Xvfb with lavapipe. Each configuration was run twice, in separate sessions, on 2026-11-26. The first run is the one in the table; the second is in the spread column. Code state: the zip with renderer E3.2 to E3.4.

Scene: 10,000 unit-half-size cubes on a 100 by 100 grid in the XZ plane, spacing 1.2. Camera at (0, 12, 70) looking at the origin. 30 warm-up frames are excluded. `sync on` waits for the GPU after every frame, so the frame time includes execution.

| mode | cull | opaque pass (µs, CPU record) | cull (µs, CPU) | frame (µs, synced) | drawn | frame, second run (µs) |
|---|---|---:|---:|---:|---:|---:|
| batched | off | 4.3 | 0.0 | 24,720 | 10,000 | 23,718 |
| batched | on | 3.3 | 127.7 | 21,057 | 5,068 | 24,244 |
| unbatched | off | 141.5 | 0.0 | 26,999 | 10,000 | 32,382 |
| unbatched | on | 75.7 | 115.4 | 22,612 | 5,068 | 28,144 |

What it shows:
- Batching cuts the CPU cost of the opaque pass by a factor of about 33 to 40 in both runs (4.3 against 141.5 µs, and 3.6 against 199.4 µs in the second run). This is a stable result.
- Culling drops half the instances (5,068 of 10,000 remain for this camera), at a CPU cost of about 115 to 170 µs per frame. The CPU cost is about 12 to 17 ns per instance.
- The frame-time effect of culling is not separable from run-to-run variance on this host. Batched frame times were 24.7 and 23.7 ms without culling, and 21.1 and 24.2 ms with it. The unbatched frames moved by 5 ms between runs.

What it does not show:
- Real GPU cost. Lavapipe runs the vertex and fragment work on the CPU, and the cube is rasterised with overdraw. Real hardware will weight these differently.
- The benefit of culling on GPU work, which is what E3.4 is for. A stable number needs GPU timestamp queries (open, see D-93).

Compared with: the debug-build run in B-006 (not comparable, because the cull code was unoptimised there).

## B-005: shader hot reload latency (release)

Command: `target/release/render-bench --scenario reload`, under Xvfb with lavapipe. Run 2026-11-26. The scenario writes a valid edit at frame 60, a broken edit at frame 120, and the original text at frame 180, and polls `reload_shader` each frame.

| check | result |
|---|---|
| valid edit visible after | 1.3 ms (limit: 300 ms) |
| broken edit rejected with a compile error | yes |
| renderer kept drawing during the broken window | yes (one instance drawn in each of those frames) |
| restoring the original text | from the content-hash cache, with no recompile |

What it shows: an edit appears within the next frame, well inside the 300 ms limit in the plan. A broken edit leaves the previous pipeline in place. Going back to a previous version reuses the cached pipeline.

What it does not show: the latency under a heavy frame load. The poll runs once per frame, so a 60 Hz display with a slow frame adds at most one frame of delay.

Compared with: the debug-build run in B-006 (4.2 ms and 3.6 ms; not comparable).

## B-006: debug-build runs of the dense scene and reload scenario (history)

Debug build, so the cull and shader code are unoptimised. Kept so the record is complete. Not comparable with B-004 or B-005.

| run | mode | cull | sync | opaque (µs) | cull (µs) | frame (µs) | drawn |
|---|---|---|---|---:|---:|---:|---:|
| dense | batched | off | off | 22.0 | 0.0 | (not measured) | 10,000 |
| dense | unbatched | off | off | 428.3 | 0.0 | (not measured) | 10,000 |
| dense | batched | on | off | 16.0 | 1,412.3 | (not measured) | 5,068 |
| dense | batched | off | on | 15.6 | 0.0 | 26,082 | 10,000 |
| dense | batched | on | on | 15.7 | 1,571.9 | 25,794 | 5,068 |
| dense | unbatched | off | on | 388.7 | 0.0 | 46,584 | 10,000 |
| dense | unbatched | on | on | 396.2 | 1,567.3 | 37,304 | 5,068 |

Reload, debug build: edit visible after 4.2 ms and then 3.6 ms; broken edit rejected in both.

## History: superseded single runs (not comparable)

These runs used single measurements, which the host's variance makes unreliable. They are kept so the record is complete. Do not compare them with B-001.

| run | plain (ns/obj) | half scripted | all scripted | plain, large | state |
|---|---:|---:|---:|---:|---|
| first baseline | 7.9 | 36.2 | 59.4 | 11.0 | before the review pass |
| after review pass | 10.0 | 21.9 | 51.7 | 15.2 | single run, noisy |
| repeat | 9.9 | 23.8 | 59.1 | 12.4 | single run, noisy |
| repeat | 9.4 | 17.5 | 36.8 | 12.4 | single run, noisy |
| after services change | 9.5 | 19.7 | 56.0 | 16.9 | single run, noisy |

## Quality checks that are not benchmarks

- Windows type-check: `cargo check --workspace --target x86_64-pc-windows-msvc --locked` passes (Rust 1.89 standard library for Windows installed from the conda-forge package). This is a compile check only; nothing has been run on Windows.

- `crates/weave-heart/tests/allocations.rs`: counts heap allocations on the test thread across 100 steady-state frames of 200 loaded objects with scripts. The count must be zero. It was confirmed to catch a deliberate allocation.

## Template for new entries

```
## B-00N: <what is measured>

Command: <exact command>. Run <date>. Code state: <zip or commit>, and what changed since the last entry.

| case | ... | result |
...

What it shows: ...
What it does not show: ...
Compared with: B-00M (same environment), or "no comparable entry".
```
