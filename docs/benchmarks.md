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

## B-017: GPU-driven culling, depth prepass, and PBR, A/B against the original (lavapipe)

Command, run on the same session and machine for every row: `render-bench --scenario dense --count 10000 --cull on --sync on --frames 200` at the default 800x600 window, one light, two runs per row. The original build is the uploaded zip before the renderer pass. The original's render-bench was built with shadows switched off in its source for the "shadows off" rows, and the new build's was switched off the same way. Scratch copies were used for the "prepass off" row. Code state: the renderer pass that adds GPU culling (`cull` pass with indirect draws), the depth prepass (`depth` pass), GGX shading with ACES tone mapping, and the shadow-caster fix.

Environment: as in the header of this file. Lavapipe runs the GPU work on the same CPU, so every number is CPU-bound and none is a real GPU measurement. This session is slower than B-004's: the original build measures 70 to 71 ms per frame here, against 21 to 24 ms recorded in B-004.

| build | shadows | prepass | frame (µs), run 1 | frame (µs), run 2 | gpu opaque (µs), run 1 | gpu opaque (µs), run 2 | cull (µs, CPU) | drawn |
|---|---|---|---:|---:|---:|---:|---:|---:|
| original | on | n/a | 71,072 | 70,329 | 41,949 | 41,636 | 191 to 214 | 5,068 |
| original | off | n/a | 39,143 | 41,767 | 33,460 | 35,478 | 214 to 223 | 5,068 |
| new | on | on | 94,033 | 80,796 | 45,174 | 37,839 | 729 to 779 | 5,068 |
| new | off | on | 61,628 | 63,936 | 37,202 | 38,558 | 720 to 729 | 5,068 |
| new | off | off | 61,050 | 57,739 | 54,112 | 51,138 | 719 to 740 | 5,068 |
| new | on | off | 93,185 | 94,059 | 62,918 | 63,424 | 719 to 740 | 5,068 |

What it shows:
- The GPU culling matches the CPU culling: 5,068 instances drawn in every row, the same count as B-004 for the same camera.
- With shadows off, the new renderer is about 50 percent slower per frame than the original on lavapipe (62 to 64 ms against 39 to 42 ms). This is a regression on this host, not a speed-up.
- The depth prepass is close to neutral on this scene. Without it, the opaque pass takes about 52 to 54 ms, against about 37 to 39 ms with it, but the total frame changes by about 3 ms. So the prepass pays for its extra geometry pass on this scene, and it's kept because its saving grows with shading cost.
- Shadows cost about 20 to 30 ms per frame in the new renderer on lavapipe. The shadow pass now draws every caster inside the light frustum, not only the ones the camera sees. That is the correctness fix, and its cost on this scene has not been split out by pass.
- The CPU cull time rose from about 200 µs to about 730 µs, because the new path uploads all 10,000 instances as 96-byte records every frame, instead of culling on the CPU first.

What it does not show:
- Whether GPU-driven culling is faster than CPU culling on a real GPU. On lavapipe the culling compute runs on the same CPU as everything else, so the trade-off cannot be measured here. The CPU saving is expected to matter only at object counts far above 10,000.
- The visual quality of PBR on a real display. The showcase frame was checked on lavapipe and looks correct; the metallic setting of the sphere (0.9) is a judgement call.

Compared with: B-004 (same scene and flags, earlier host state, not comparable in absolute terms). Its batched, culling-on row is the nearest comparison for the culling count only.

## B-018: two-phase Hi-Z occlusion and discrete LOD (lavapipe)

Code state: the renderer pass that adds the depth pyramid (HZB), two-phase occlusion culling (`cull`, `depth`, `hzb`, `cull2`, `depth2`), and per-instance LOD selection in the cull shader. The showcase's sphere has four levels (256x128, 128x64, 48x24, 16x8 tessellation).

Environment: as B-017. Same session, lavapipe, 800x600 default window for the benchmark, 1024x640 for the showcase captures.

Occlusion, with a wall: `render-bench --scenario dense --count 10000 --cull on --wall on --sync on --frames 200`, one run each, occlusion on and off. The wall is one 120 x 40 slab placed between the camera and the grid, so most of the grid is hidden behind it.

| occlusion | drawn (last frame) | gpu opaque (µs) | frame (µs) |
|---|---:|---:|---:|
| on | 418 | 24,249 | 66,419 |
| off | 5,069 | 39,246 | 91,069 |

Occlusion, without the wall (same command without `--wall`): drawn 5,068 in both settings, frame 88 to 90 ms with occlusion on, 87 ms with it off, in two runs each. The dense grid is a flat sheet of small cubes that barely hide each other, so there is little to cull. This row confirms the feature costs nothing measurable when it doesn't help.

Correctness, by image: the showcase was captured with `--still on` (the sphere's rotation frozen) and occlusion on and off. The two frames are pixel-identical: 0 differing pixels out of 1,024,000.

What it shows:
- The occlusion culls what the depth buffer says is hidden. With a large occluder, 92 percent of the instances are removed before the opaque pass.
- Image output is unchanged, for the scene tested.

What it does not show:
- Anything about a real GPU. On lavapipe the culling compute runs on the CPU, so the saving in GPU time here is a CPU saving too.
- LOD cost or quality, separately. The showcase frame looks right with LOD selected, but no per-LOD triangle counts were recorded, so the LOD effect on time is not measured.
- Resizing the window. The HZB and its bind groups are rebuilt on resize, but no test exercises that path.
- Temporal behaviour when the camera moves. The two-phase design should draw newly visible objects in phase two, but a moving camera was not captured, so popping is not ruled out.

Compared with: B-017, same flags with the wall added. The no-wall dense rows match B-017's new-renderer rows within run-to-run spread.

## B-019: instance record 96 to 64 bytes, delta upload, shadow map cache (lavapipe)

Code state: `renderer.rs` with the 64-byte instance record (D-R12), delta upload of instance data, and the shadow map cache with finest-LOD shadow casters (D-R11). Compared with the code before these changes, which is the tree at B-018.

Environment: this session. 1 vCPU, 3 GB RAM, Ubuntu 24.04, rustc and cargo 1.89.0, wgpu 30.0.2, Mesa lavapipe (CPU Vulkan) under Xvfb. No GPU. The host is shared, so repeated runs vary.

Dense scene, cull on, sync on: `render-bench --scenario dense --count 10000 --mode batched --cull on --sync on --frames 200 --size 1920x1080`, two runs each.

| build | frame (µs) | upload per frame | shadow reused |
|---|---:|---:|---|
| before | 197,613 and 196,650 | 960,000 B (10,000 × 96, computed) | not applicable |
| after | 168,647 and 167,814 | 0 B | 170 of 170 |

Dense scene, cull off, sync on: same command with `--cull off --frames 150`, two runs each.

| build | frame (µs) | shadow reused |
|---|---:|---|
| before | 174,582 and 172,758 | not applicable |
| after | 140,298 and 140,902 | 120 of 120 |

Drawn counts are unchanged (5,827 with culling, 10,000 without), so visibility did not change.

Image check: `showcase --size 1024x640 --still on --shadows on --occlusion on`, captured from the X root window. Of 655,360 pixels, 755 differ by at least 2 levels, 143 by more than 4, 12 by more than 20, and the maximum is 37. Most single-level differences come from 8-bit color packing. The larger differences cluster in rows 400 to 480, around the sphere's shadow edge, and come from the shadow LOD now being fixed at the finest level.

What it shows:
- Static scenes stop uploading instance data and stop re-rendering the shadow map. The frame saving in this software-rasterized setup is about 15 percent with culling and about 19 percent without.
- The instance record is one third smaller, and output is unchanged except for the small shadow-edge difference above.

What it does not show:
- Anything about a real GPU. Lavapipe rasterizes on the CPU, so the saving in shadow and raster work is exaggerated relative to hardware where those passes are cheap.
- Cost when the scene changes every frame. The changed-run path was tested only by unit tests (`upload_tests`), not timed. The fallback path compares about 640 KB per frame on the CPU.
- Moving lights or moving casters with the cache in use. The cache is invalidated by any change in light matrix, caster bytes, or batch layout, but a moving-scene capture was not run.
- Multi-level meshes, where forcing the finest shadow LOD costs extra triangles in the shadow pass. The benchmark meshes are single-level.

Compared with: B-018 (same command and machine) for the frame times. The before rows are the original tree, rebuilt from the same source.

## B-020: dynamic resolution, frame-time controller and upscale pass (lavapipe)

Code state: `weave-eye` with the dynamic resolution controller (`dynres.rs`, D-R13), the scene rendered to an internal target and upscaled to the swapchain by a bilinear pass, and depth and HZB sized to the internal target. Compared with the code at B-019.

Environment: as B-019. 1 vCPU, Mesa lavapipe under Xvfb, rustc 1.89.0, no GPU.

Command: `render-bench --scenario dense --count 10000 --mode batched --cull on --sync on --size 1920x1080`, with the dynamic resolution flags shown.

| setting | frame (µs) | render scale (average) |
|---|---:|---:|
| `--dynres off`, 120 frames | 167,471 | 1.00 |
| `--dynres on --budget-ms 400`, 120 frames | not recorded | 1.00 |
| `--dynres on --budget-ms 150 --min-scale 0.5`, 200 frames | 113,425 | 0.54 |
| `--dynres on --budget-ms 100 --min-scale 0.5`, 200 frames | not recorded | 0.50 |

The average scale for the 150 ms budget includes the first frames, which run at full scale before the controller reacts.

Image check: a 1920×1080 root-window capture during a scaled run (budget 100 ms, scale held at 0.5) and during a full-resolution run. Mean brightness 99.5 against 98.2, and the scaled frame has no black regions. This is a sanity check, not a pixel comparison; the scaled image was not compared pixel by pixel.

Controller tests (6 unit tests in `dynres.rs`): full scale when under budget, immediate drop on one heavy frame, never below the minimum, settling near the budget with no oscillation, recovery to full scale when load drops, and every scale value on the 0.05 grid.

What it shows:
- The controller works end to end. It holds full scale when the budget is loose, drops to the minimum when the budget is unreachable, and settles between the two.
- The scaled path runs without validation errors and produces a visible image.

What it does not show:
- Frame time against scale on real hardware. On lavapipe, the cube work (culling, vertex, and fixed per-frame costs) does not shrink with pixel count, so frame time falls far less than scale squared. At scale 0.5 the frame still takes about 110 ms. On a GPU where the cost is mostly per-pixel, the reduction should be larger.
- Quality of the bilinear upscale against native resolution. No sharpening or temporal filter is in place yet.
- Behaviour with GPU timestamps. The controller uses frame interval, which matches GPU time only when the benchmark waits for the GPU (`--sync on`) or presentation is blocking.
- Behaviour on window resize while scaled. The targets are rebuilt on resize, but no test exercises that path.

## B-021: GPU-timestamp controller, sharpened upscale, and blit cost (lavapipe)

Code state: the controller takes GPU frame time from timestamps (the sum of the timed passes from the first timed point to the last, including the upscale pass), and it ignores samples until they reflect the current scale (RING + 1 frames after a change). The upscale pass uses a contrast-adaptive sharpening filter (AMD CAS-style, five taps). Compared with B-020, which used frame interval.

Environment: as B-019 and B-020.

Edge energy, same scaled scene (budget 100 ms, scale held at 0.5). The Laplacian mean of the captured frame, as a sharpness measure:

| output | edge energy | mean luma |
|---|---:|---:|
| full resolution (B-020 capture) | 17.7 | 99.1 |
| scaled, bilinear only (B-020 capture) | 6.1 | 100.4 |
| scaled, with sharpening | 9.8 | 99.5 |

Blit cost, pinned at scale 0.5 (`--dynres on --budget-ms 1 --min-scale 0.5 --frames 120`, the controller stays at the minimum):

| upscale filter | frame (µs) | gpu opaque (µs) |
|---|---:|---:|
| sharpening | 177,788 | 56,671 |
| bilinear only | 117,052 | 53,286 |

Controller, GPU timestamps: `--dynres on --budget-ms 150 --min-scale 0.5 --frames 240` gives frame 175,820 µs at scale 0.50, and `--budget-ms 400 --frames 120` gives frame 189,364 µs at scale 1.00.

What it shows:
- The sharpening filter raises edge energy about 60 percent over bilinear at the same scale, and recovers part of the gap to full resolution.
- On lavapipe the sharpen pass adds about 60 ms per frame at 1080p. The controller is correct to count it, because it is part of the GPU work.
- The GPU-timestamp controller now follows GPU work, and it no longer reacts to stale samples.

What it does not show:
- A real GPU cost. Lavapipe runs fragment shaders on the CPU, so the 60 ms is far larger than a five-tap filter over 2 million pixels would cost on a GPU. This needs a measurement on hardware.
- Why the GPU-timestamp controller drops to the minimum at a 150 ms budget, while the interval controller held about 113 ms in B-020. The pass sum at the minimum scale stays above the 85 percent raise threshold, so the controller correctly keeps the scale low. The difference from B-020 comes mostly from the sharpen cost, which the interval controller saw too but which was smaller in that run. This was not separated further.
- The cull compute pass is not timed (it has no timestamp writes), so its cost is excluded from the controller input.
- CPU-side cost. The controller watches GPU time only, so a CPU-bound frame does not lower the scale.

## B-022: static-scene upload, shadow LOD from the light, and shadow gating (lavapipe, this session)

Session: 1 vCPU, Mesa lavapipe under Xvfb, rustc and cargo 1.89.0, release builds from the uploaded tree plus the changes in D-R16 to D-R18. Single runs unless stated, so variation is about ±10 percent (see B-004 for the run-to-run spread on this host).

Dense scene, the static case, same command before and after the upload change:
`render-bench --scenario dense --count 10000 --cull on --sync on --frames 200`, 800x600.

| build | frame (µs) | CPU batch setup and upload per frame (µs) | upload per frame |
|---|---:|---:|---:|
| before the change | 51,711 | 649 | 0 B |
| after the change | 53,229 | 19 | 0 B |

The frame times are within noise of each other; GPU opaque time did not change. The CPU saving is real and removes about 0.6 ms of per-frame work on the CPU, which matters once the scene is larger or the CPU is busy. Note that the value labelled `cull` in the output is now the per-frame CPU time for batch setup and upload; the CPU culling it used to include is gone from the frame path.

Dense scene, per-pass GPU time, after the change, 120 frames: depth prepass 10,385 µs, opaque 28,756 µs, clear 867 µs, shadow pass 324 µs (cache reused 90 of 90 frames). The shadow cache is what keeps the shadow pass at a few hundred microseconds here.

Showcase, shadows on, 800x600, 120 frames, shadow pass GPU time (the sphere rotates, so the cache does not hit):

| build | shadow pass (µs) | opaque (µs) |
|---|---:|---:|
| before the shadow LOD change | 20,793 | about 22,000 |
| after, first run | 8,680 | 22,052 |
| after, second run | 9,878 | 24,403 |

The shadow LOD change (D-R17) picks the caster's LOD from its size in shadow-map pixels, with thresholds twice the camera's. The shadow pass draws about one quarter of the triangles for the sphere. The captured frame shows no visible faceting at the shadow edges.

Shadow lookup gating (D-R18), dense scene, 120 frames: opaque 29,192 µs before and 28,756 µs after. That is within noise, so this change is not claimed as a measured speed-up. It removes work for surfaces facing away from the key light on real hardware, where texture fetches are the cost.

Regression checks:
- Reload scenario (the shader changed): edit applied in 13.4 ms, broken edit rejected, restore from cache. The first run after the shader change failed for a reason in the benchmark, not the renderer: its edit targeted a return line that no longer existed, so the test "edit" was the original text. The harness now asserts that its edit changes the shader.
- Showcase image, shadows on, after the gating change: shadows match the earlier capture (sphere, cubes, and the floor shadows are in the same places).
- CPU heart benchmark (`heart-bench`): within the range recorded in B-001 and B-004. The heart code did not change.

What it does not show:
- Anything about a real GPU. Lavapipe executes the fragment and vertex work on the CPU, so the per-pixel shading (about 29 ms for 5,068 cubes) is the dominant cost here and is not representative.
- The shadows-off comparison that B-017 reports. `render-bench` has no shadows switch, so that row was not re-run in this session.

Compared with: B-017 (the same scene and flags, earlier host state), B-018 (occlusion), and B-019 (the upload path that this change refines).

## B-023: real GPU, reported by the project owner (not measured in this environment)

Machine: a laptop with an NVIDIA GeForce GTX 1050, driving a 60 Hz display. Reported result: the showcase runs at 60 frames per second.

What it shows: the current renderer meets its 60 fps target on real hardware. The lavapipe entries above (B-004 to B-022) run the GPU work on the CPU, so they overstate the cost of every pass. Real-GPU numbers will be lower than those tables.

What it does not show: headroom. A 60 Hz display caps the frame rate at 60, so a frame that takes 4 ms looks the same as one that takes 15 ms. Headroom needs the frame time with the cap removed.

Commands to measure headroom on the same laptop (send the output of each):

```
target\release\render-bench --scenario dense --count 10000 --cull on --sync on --frames 300
target\release\showcase --size 1920x1080 --frames 300
target\release\showcase --size 1920x1080 --frames 300 --extra-lights 512 --extra-range 1.2
```

Add the output of the pass report (printed at the end of the showcase run) to the entry, and record the GPU name, driver version, and the display's refresh rate.

## B-024: HD lighting, ambient occlusion cost, armadillo scene (lavapipe, this session)

Scene: `showcase --size 1920x1080 --frames 40 --model assets/external/armadillo.obj`, release build, Mesa lavapipe under Xvfb, 1 vCPU. The armadillo is the Stanford model from docs/models.md, fitted to radius 1.6, 100,000 triangles.

| pass | GPU average, AO on (µs) | GPU average, AO off (µs) |
|---|---:|---:|
| ao (half resolution, 12 samples) | 50,764 | 128 |
| ao_blur (4x4 box) | 16,836 | 120 |
| ao (half resolution, 8 samples) | 29,062 | 128 |
| ao_blur (3x3 box) | 10,798 | 120 |
| opaque, 8 samples | 146,825 | 148,373 |

What it shows: the eight-sample, three-by-three version cuts the AO cost from about 67 ms to about 40 ms per frame on this software driver, and the image stays clean. The opaque pass is unchanged, since it does the armadillo's shading rather than the occlusion.

Changing the per-sample depth reconstruction to a direct depth-to-view conversion (shader change in ao.wgsl) did not change the AO cost (50,764 µs before and after with twelve samples). The cost is the number of samples per pixel, not the reconstruction, so the reconstruction change is kept for clarity but is not claimed as a speed-up.

Quality: the occlusion darkens the gaps between the legs and under the belly and is subtle at this scale. The contact under the feet is hard to see at 1280x720. Stronger settings (radius 0.9, strength 2.2) were checked against the default and were judged modest, so the default is kept.

What it does not show: real-GPU costs. On a GPU, these passes are much cheaper than on this driver, and the GTX 1050 result in B-023 is the reference for real hardware.
