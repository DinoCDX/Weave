# Benchmark log

Every benchmark run for this project is recorded here, with the command, the environment, the raw result, and what it does and does not show. A change that claims a speed effect must add an entry first. Entries are never edited after the fact. A later run goes in as a new entry, and it names the entry it replaces.

## Environment for all entries below

- Host: virtual machine (Firecracker) on a shared host, 1 vCPU, Intel Xeon @ 2.30 GHz, 4 GB RAM, Linux 6.18.
- Toolchain: rustc and cargo 1.89.0 (the pinned version), wgpu 30.0.2, glam 0.34.3, hecs 0.11.2.
- GPU: none. Vulkan runs on Mesa's software driver (lavapipe) on Xvfb. Renderer entries from B-007 on report GPU time from timestamp queries, which lavapipe executes on the CPU; they measure the GPU work of that software device, not a physical GPU.
- Resolution: entries state the window size. Unless stated, the window is 800x600. Entries marked 1080p use 1920x1080.
- The host is shared, so identical runs vary. Single runs are not reliable here (see B-000).

## How to run

```
cargo run --release -p heart-bench                      # B-001 heart tick cost
target/release/showcase --size 1920x1080 --frames 120 --extra-lights 0   # B-010
target/release/render-bench --scenario dense --count 10000 --lights 16 --mode batched --cull off --sync off --frames 120 --size 1920x1080   # B-008
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

## B-007: renderer GPU time, 800x600, light count (release, timestamp queries)

Command: `target/release/render-bench --scenario dense --count 10000 --lights <N> --mode batched --cull off --sync off --frames 150`, under Xvfb (800x600) with lavapipe. Run 2026-11-26, same build as B-004 plus the light shader. Window 800x600.

| lights | GPU opaque (ms) | CPU opaque (µs) | GPU clear (µs) |
|---:|---:|---:|---:|
| 1 | 26.7 | 3.3 | 840 |
| 4 | 37.6 | 4.4 | 975 |
| 8 | 50.9 | 5.4 | 1,133 |
| 16 | 71.9 | 4.0 | 1,017 |

What it shows: GPU opaque time rises roughly linearly with the light count, about 3 ms per extra light at this density and resolution. The CPU cost barely moves, because lights are a shader cost.
What it does not show: the cost on real hardware, which has different fragment throughput.
Compared with: B-008 (same scene at 1080p, one and sixteen lights).

## B-008: renderer GPU time, 1920x1080 (release, timestamp queries)

Command: `target/release/render-bench --scenario dense --count 10000 --lights <N> --mode batched --cull <on|off> --sync off --frames 120 --size 1920x1080`, under Xvfb (1920x1080) with lavapipe. Run 2026-11-26.

| case | lights | cull | drawn | GPU opaque (ms) | GPU clear (ms) | CPU opaque (µs) | cull CPU (µs) |
|---|---:|---|---:|---:|---:|---:|---:|
| 1080p, cull off | 1 | off | 10,000 | 58.4 | 3.48 | 4.5 | 0.0 |
| 1080p, cull on | 1 | on | 5,827 | 58.8 | 4.31 | 4.4 | 148.3 |
| 1080p, 16 lights | 16 | off | 10,000 | 188.5 | 4.57 | 5.4 | 0.0 |

What it shows:
- At 1080p, culling halves the drawn instances (10,000 to 5,827) but leaves the GPU opaque time unchanged (58.4 ms and 58.8 ms). The GPU time is set by the pixels the cubes fill, not by vertex work, so skipping off-screen cubes does not help this scene. The cull CPU cost is about 148 µs per frame.
- Lighting dominates at this resolution: sixteen lights take 188.5 ms against 58.4 ms for one, about 3.2 times the GPU time for 16 times the lights.
What it does not show: how a physical GPU weights overdraw. Culling should pay off when much of the scene is off-screen and the view is not filled with overdraw; this scene does not test that.
Compared with: B-004 and B-007 at 800x600.

## B-009: batched and unbatched GPU time, 800x600 (release, timestamp queries)

Command: `target/release/render-bench --scenario dense --count 10000 --mode <batched|unbatched> --cull off --sync off --frames 200`, under Xvfb (800x600) with lavapipe. Run 2026-11-26.

| mode | GPU opaque (ms) | CPU opaque (µs) | drawn |
|---|---:|---:|---:|
| batched | 27.9 | 3.2 | 10,000 |
| unbatched | 31.7 | 124.8 | 10,000 |

What it shows: batching cuts the CPU recording cost of the opaque pass by about 39 times in this run (3.2 against 124.8 µs). The GPU time differs by about 12%, so batching saves CPU much more than it saves GPU work on this device.
What it does not show: a physical GPU, where per-draw overhead on the GPU side may matter more.
Compared with: B-004 (same scene, sync on, which shows a similar CPU gap).

## B-010: showcase light count, 1920x1080 (release, timestamp queries)

Command: `target/release/showcase --size 1920x1080 --frames <F> --extra-lights <N>`, under Xvfb (1920x1080) with lavapipe. Run 2026-11-26. Code state: the zip with the mesh and light changes (64-light limit, batches, showcase).

Scene: a lobed sphere of 256 by 128 segments (65,536 triangles, 33,153 vertices, 32-bit indices), a ground plane, and six cubes. Lights: key (directional), flood (wide spot, 62 degree outer cone), and accent (teal point light), plus `N` extra small point lights. The averages include every frame that returned a timestamp.

| total lights | frames | GPU opaque (ms) | GPU clear (ms) | CPU opaque (µs) |
|---:|---:|---:|---:|---:|
| 3 | 120 | 54.4 | 3.49 | 4.4 |
| 19 | 120 | 135.5 | 3.45 | 4.2 |
| 43 | 120 | 137.7 | 4.29 | 4.1 |
| 64 | 80 | 134.4 | 3.88 | 4.7 |
| 64 | 120 | 141.5 | (not recorded) | 4.7 |

What it shows: moving from three lights to nineteen costs about 81 ms of GPU time on this device. Beyond nineteen, the time stays flat at 134 to 142 ms, within run-to-run variation.

What it does not show: why the cost levels off. This is the open question below.

Open question: B-011 shows the same plateau on the dense cube scene. On lavapipe, GPU time stops rising after about 16 lights. The loop in the shader runs once per light, so a linear rise is expected. A physical GPU must be measured before the flattening is treated as a property of the renderer.

## B-011: dense cube scene, light count above 16, 800x600 (release, timestamp queries)

Command: `target/release/render-bench --scenario dense --count 10000 --lights <N> --mode batched --cull off --sync off --frames 150`, under Xvfb (800x600) with lavapipe. Run 2026-11-26.

| lights | GPU opaque (ms) |
|---:|---:|
| 16 | 71.9 (from B-007) |
| 32 | 72.9 |
| 64 | 71.2 |

What it shows: the same plateau as B-010, on a different scene. Above sixteen lights, the GPU time did not change.
What it does not show: whether the plateau comes from the renderer or from the software driver. See the open question under B-010.

## B-012: clustered lighting, 1920x1080 (release, timestamp queries)

Command: `target/release/showcase --size 1920x1080 --frames <F> --extra-lights <N> --extra-range <R>`, under Xvfb (1920x1080) with lavapipe. Run 2026-11-26. Code state: the zip with clustered lighting (16 by 9 screen tiles, 16 depth slices, up to 4096 point and spot lights in a storage buffer, 4 directional lights in the uniform).

Lights: the three named lights (key, flood, accent) plus `N` extra point lights. "Big" extra lights have range 6 and sit 4 to 8 units from the centre, so each covers most clusters. "Small" extra lights have range 1.2 and are spread the same way.

| case | total lights | range | frames | GPU opaque (ms) | per-pixel path, same case (B-010) |
|---|---:|---|---:|---:|---|
| clustered, 3 lights (rectangle bounds, superseded by B-013) | 3 | (named) | 40 | 184.5 | 54.4 |
| clustered, 64 big lights (rectangle bounds, superseded by B-014) | 64 | 6 | 20 | 935.8 | 134.4 |
| clustered, 512 small lights (rectangle bounds, superseded by B-014) | 512 | 1.2 | 20 | 347.2 | not run |
| clustered, 2,048 small lights (rectangle bounds, superseded by B-014) | 2,048 | 1.2 | 12 | 1,114.7 | not run |

What it shows:
- Small, local lights scale. Going from 512 to 2,048 small lights adds about 0.46 ms of GPU time per light, and the cost grows with the lights that touch each pixel, not with the total. The old per-pixel loop could not run these counts at all: it stopped at 64.
- Large lights do not scale. A light whose range covers most clusters is put in most clusters, so each pixel tests many lights. At 64 big lights the clustered path is about 7 times slower than the per-pixel loop.
- The clustered path has a fixed overhead. At three lights it costs 184 ms, against 54 ms for the per-pixel loop. That overhead comes from the per-pixel cluster lookup and the storage-buffer reads, on lavapipe.

What it does not show: how a physical GPU weights storage-buffer reads against ALU work. The overhead and the large-light result may both change on real hardware.

Compared with: B-010 (the per-pixel loop). The per-pixel loop is faster for few lights and cannot exceed 64 lights.

## B-013: direct and clustered light paths at small counts, 1920x1080 (release, timestamp queries)

Command: `target/release/showcase --size 1920x1080 --frames <F> --extra-lights <N> --extra-range 1.2 --light-path <direct|clustered>`, under Xvfb (1920x1080) with lavapipe. Run 2026-11-26. Code state: tight sphere-versus-cluster bounds (D-113). "Lights" counts the storage lights: the flood and accent lights plus the extras. The key light is directional and sits in the uniform.

| storage lights | direct GPU opaque (ms) | clustered GPU opaque (ms) | faster |
|---:|---:|---:|---|
| 2 (named lights only) | 52.3 | 81.0 | direct |
| 4 | 68.2 | 79.5 | direct |
| 8 | 100.6 | 80.5 | clustered |
| 16 | 167.6 | 85.3 | clustered |

What it shows: the crossover is between four and eight storage lights on this device. Below four, looping over every light is cheaper than the cluster lookup; above, the cluster lookup wins and the gap grows. The threshold is set to four (D-114).

What it does not show: the crossover on a physical GPU. Storage-buffer reads and ALU cost are weighted differently there.

## B-014: clustered path after tight sphere bounds, 1920x1080 (release, timestamp queries)

Command: as B-012, with `--light-path clustered`. Run 2026-11-26.

| case | total lights | range | frames | GPU opaque (ms) | B-012 (rectangle bounds) (ms) |
|---|---:|---|---:|---:|---:|
| 64 big lights | 64 | 6 | 20 | 475.3 | 935.8 |
| 512 small lights | 512 | 1.2 | 20 | 238.1 | 347.2 |
| 2,048 small lights | 2,048 | 1.2 | 12 | 713.9 | 1,114.7 |

What it shows: replacing the rectangle test with a sphere test against each cluster's frustum cell removes about half the cost for large lights (936 to 475 ms) and about a third for small ones at the top count (1,115 to 714 ms). The cost still grows with the number of clusters each light covers, so large lights remain the expensive case.

What it does not show: the cost on a physical GPU.

Compared with: B-012 (same scenes, rectangle bounds).

## B-015: shadow cost on the key light, 1920x1080 (release, timestamp queries)

Command: `target/release/showcase --size 1920x1080 --frames 40 --extra-lights 0 --shadows <on|off>`, under Xvfb (1920x1080) with lavapipe. Run 2026-11-27. Code state: shadow map 2048 by 2048, depth-only pass, four hardware-filtered comparison taps in the lit shader, and a uniform early-out when shadows are off.

| shadows | shadow pass GPU (ms) | opaque GPU (ms) |
|---|---:|---:|
| on | 16.3 | 72.6 |
| off | 2.6 (empty pass) | 55.6 |

What it shows: the shadow map costs about 16 ms per frame on this device, and the lookup in the lit shader about 17 ms. The off case is 55.6 ms, against 52 to 56 ms for the same scene before shadows existed, so the switch adds no cost when off.

B-016 (below) records the first version, which had nine taps and no early-out, and was 85 ms with shadows off. That was a real regression, found by this benchmark, and fixed.

Compared with: B-013 (the same scene before shadows; direct path, three lights, 52 ms).

## B-016: shadow lookup before and after the tap reduction (release)

| version | shadows | opaque GPU (ms) |
|---|---|---:|
| nine taps, no early-out | off | 85.1 |
| nine taps, no early-out | on | 84.7 |
| four taps, uniform early-out | off | 55.6 |
| four taps, uniform early-out | on | 72.6 |

What it shows: the lookup was computed for every pixel even with shadows off. The uniform early-out and the reduction to four taps removed about 30 ms when shadows are off, and about 12 ms when they are on.

Quality: the four-tap version was checked visually (`showcase-frame-1080p` and the 720p capture); edges stay soft and there is no visible acne.

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
