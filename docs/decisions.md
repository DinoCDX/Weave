# Decision records

Phase 0 updates to plan revision 8, Section 14. The full table stays in the plan.

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-02 | Graphics | `wgpu` pinned to `=30.0.2` now, not at Phase E3.1, because the Phase 0.2 triangle needs it. 30.0.2 is the latest non-yanked release on crates.io at the time of pinning, and its declared minimum Rust version is 1.87.0 (the toolchain is now 1.89.0, see D-16). | Confirmed; pin moved earlier |
| D-04 | Entity storage | `hecs` 0.11.2 kept. The dependency is added in the first phase that uses it. | Confirmed; not yet a dependency |
| D-13 | Object identity | Stable ID is written in the object file. `hecs::Entity` is never stored in a file. | Confirmed by the Phase 0.3 check |
| D-16 | Toolchain | Rust 1.89.0 pinned in `rust-toolchain.toml`, with `rustfmt` and `clippy`. Raised from 1.87.0 after a build showed that `hecs` 0.11.x needs 1.89. `hecs` declares 1.81, so Cargo does not catch this. | Confirmed by build |
| D-17 | Window for the triangle | `winit` 0.30. Version 0.31 is still a beta release. | Confirmed |

## Phase 0.3 check: hecs entity layout

`hecs::Entity` is a struct of a `u32` index and a `NonZeroU32` generation, from `hecs` 0.11.2 `src/entities.rs`. The index is reused after a despawn, and the generation changes each time. The value is valid only in the running `World`.

Consequences:
- An `Entity` cannot be an object's identity in a file. D-13 stands: each object has its own stable ID that is written in the file.
- At load time, the loader maps each stable ID to the `Entity` it spawns. A map from stable ID to `Entity` lives in WeaveHeart.
- A stale `Entity` after despawn is detected by `hecs` through the generation. This matches the rule in plan Section 5.5 that stale handles return "none" and never crash, since WeaveHeart checks the handle before use.

## Phase H1: object model

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-04 | Entity storage | `hecs` is now a dependency, pinned to `=0.11.2`. Entities are allocated and their `Transform` is stored in `hecs`. | Confirmed; in use from H1 |
| D-18 (superseded by D-48) | Name and parent location | Plan 5.10 says every object has `Name`, `Transform`, and `Parent` as components. H1 keeps `Name`, `Parent`, the children, and the sibling-name index in an index inside `ObjectWorld`, and keeps only `Transform` in `hecs`. Sibling uniqueness and stable paths are invariants of the tree, and they need lookups by name and by ID that archetype storage does not give. H2 revisits this when the derive macro defines the component schema. | Assumed; revisit in H2 |
| D-19 | Handle safety | `ObjectHandle` wraps `hecs::Entity`, so generations detect stale handles. Every public accessor returns `None`, `false`, or an error for a stale handle. Nothing panics on a stale handle. | Confirmed by H1 tests |
| D-20 | Object IDs | `ObjectId` is a `u64`. Fresh IDs are issued above every ID seen, so explicit IDs from files never collide with fresh ones. IDs are not reused within a world. | Confirmed by H1 tests |

## Phase H2 and H3: components, derive, lifecycle

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-21 | Derive crate dependency | `weave-heart` has no dependency on `weave-macros` at runtime, to honour plan 2.5. The derive is `weave_macros::Component`. `weave-heart` uses `weave-macros` only as a dev-dependency for its tests. Users depend on both crates. | Confirmed |
| D-22 | Field values | Components convert to and from a small `Value` enum (bool, i64, f32, text, float arrays of length 3 or 4). Floats round-trip exactly. The file format itself is still chosen in L1. | Confirmed for H2 |
| D-23 | Schema hash | FNV-1a over the component name and, for each field, its name and kind. Computed in a `const fn`, so it is fixed at compile time. | Confirmed |
| D-24 | Frame stages | Per frame: load, enable, ready, update, late update, disable, unload, as plan 5.7. Requests made before a tick take effect in that tick at their matching stage. Enable and disable therefore take effect at their stage, so an object disabled before a tick still runs its update for that frame. | Confirmed by H3 tests |
| D-25 | Unload order | Children unload before parents, in postorder. Each enabled object gets on_disable before on_unload. Despawn unloads the subtree first, then removes it. | Confirmed by H3 tests |
| D-26 | Deferred work | Structural changes and lifecycle requests go through `CommandBuffer` and are applied at the frame boundary. Spawn is deferred, so it returns no handle; use `Runtime::spawn` when the handle is needed immediately. Stale or invalid commands are skipped and returned in `FrameReport`. The caller decides whether to log them. | Confirmed by H3 tests |
| D-27 | Component writes in stages | Components are read and written through `ObjectWorld` outside stages. Writes from inside a stage need a command, which H4 will add with scripts. | Closed by D-52 |
| D-28 | Listener | H3 defines the `Listener` trait that receives every stage. H4 replaces this with per-object scripts. | Assumed; revisit in H4 |

## Build verification

The project was built and tested with Rust 1.89.0 (`cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`). All pass. The Windows build and the DX12 triangle have not been run, because this environment is Linux only. The triangle needed one fix: `RequestAdapterOptions` in wgpu 30 has a required `apply_limit_buckets` field.

## Phase H4: scripts

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-29 | Script trait | `Script` is `Any + Send`, with default no-op hooks for the seven stages. Stored state is read back by downcasting through trait upcasting (Rust 1.86+). | Confirmed by H4 tests |
| D-30 | Per-attachment state | Each attachment is its own boxed instance. A modular script attached to N objects has N states. | Confirmed by H4 exit test (ten objects, ten states) |
| D-31 | Late attachment | Attaching to an object that is already loaded delivers `on_load`, then `on_enable` if enabled, then `on_ready` if ready, at the start of the next frame, before the load stage. | Confirmed by H4 tests |
| D-32 | Script lifetime | Scripts keep their state across unload and reload. They are removed when their object is despawned. | Confirmed by H4 tests |
| D-33 | Dispatch order | In a stage, objects run in stable ID order, and each object's scripts run in attach order. Scripts run before the `Listener` for the same event. | Confirmed by H4 tests |
| D-34 | Script writes | A script changes the world only through `ScriptContext::commands`, which applies at the next boundary. | Confirmed by H4 tests |
| D-35 | Attach and register API | `Runtime::attach`, `attach_modular`, and `register_script` are immediate calls made outside stages. Scripts attach from inside a stage through `CommandBuffer::attach` and `attach_modular`, which apply at the next boundary. | Closed by D-52 |
| D-36 | Services and failure | Services declared per script (plan 5.6) and the script-failure rule (plan 5.9) are not in H4. They belong to H5. | Open; H5 |

## Phase H5: service context and failure rule

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-37 | Service declaration | A script lists the services it uses in `type Services`, as a tuple of marker types (`Time`, `Input`, `Scene`), or `()`. Each accessor requires the matching service through a `Contains` bound, so undeclared use fails to compile, and the compiler names the declared services. Checked by a `compile_fail` doc test in `docs/services.md`. | Confirmed by H5 test |
| D-38 | Service contents | `Time` gives the frame delta and elapsed time. `Input` gives the key state set by the host through `Runtime::input_mut`. `Scene` gives read-only queries on the object tree. The object handle and `commands()` need no declaration. | Confirmed for H5; `Input` is a placeholder until weave-platform adds real events |
| D-39 | Typed hooks, erased storage | Hooks are typed by `Services`, so `Script` is not object-safe by design. The runtime stores scripts behind an internal erased trait whose blanket implementation builds the typed context. | Confirmed by H4 and H5 tests |
| D-40 | Failure rule | In debug builds, each hook call is wrapped in `catch_unwind`. A panic disables that one attachment for the rest of the run, and a `ScriptFailure` with the object path and stage goes into `FrameReport` and to stderr. Other scripts and objects keep running. In release builds the panic propagates and stops the game, as plan 5.9 says. | Confirmed by H5 tests in debug; release behaviour not run by the test suite |
| D-41 | Failure scope | The disabled attachment keeps its state and does not run its hooks again, including on_unload. | Confirmed by H5 tests |

## Phase H6: platform overrides

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-42 | Platform keys | `PlatformKey` is `Windows`, `Linux`, or `Android`. `PlatformKey::current()` is a `const fn` on `cfg!`, so the choice is compile-time. `Runtime::for_platform` takes a key explicitly, for tests and for WeaveLoom. | Confirmed by H6 tests |
| D-43 | Override component | `PlatformOverride` holds one list of modular script names per platform. Its schema is generated, and it round-trips. | Confirmed by H6 tests |
| D-44 | Applying overrides | When an object first loads, the runtime attaches the scripts listed for the runtime's platform, once per object. A missing script is reported in `FrameReport::override_errors`. Overrides for other platforms are inert at run time. | Confirmed by H6 tests |
| D-45 | Build-time stripping | `keep_only(key)` removes the other platforms' lists. WeaveLoom will call it at build time (plan 5.4), and the test shows an override is present only in its own platform's build. | Confirmed for the operation; WeaveLoom not yet built |
| D-46 | Component value overrides | Plan 5.4 also lets an override change component values per platform. Not built in H6: the change format depends on the file format, which phase L1 chooses. | Open; L1 |
| D-47 | Field kinds | Added `Vec<String>` (`TextList`) so the schema can hold lists of names. | Confirmed |

## Quality rules checked by tests

| ID | Rule | Check | Status |
|---|---|---|---|
| E3 | No heap allocation in steady-state frames | `tests/allocations.rs` counts allocations on the test thread across 100 frames of 200 loaded objects with scripts. The counter was confirmed to catch a deliberate allocation. | Confirmed for the reference scene; CI runs it on Windows and Linux |

## Improvements made to existing code

- Every stage now receives the frame's delta time. Before, load, enable, and ready passed 0.0.
- A script attached to an already-loaded object now gets the frame's delta time at its late start, not 0.0.
- Late start no longer builds a temporary list for each attachment.
- Stage functions share one `Env` for the frame's world, command buffer, input, time, and failure list. This replaces long argument lists.
- `Runtime::register_script` takes a factory that returns the script type, so user code no longer writes `Box::new`.
- `Runtime::insert_component` lets components be added without exposing `ObjectWorld` mutably, which could bypass the runtime's invariants.
- CI uses `--locked` on build, test, and clippy, so it cannot drift from `Cargo.lock`.
- `.gitattributes` forces LF endings. Without it, the Windows checkout can produce CRLF and fail `cargo fmt --check`.

## Heart completion pass

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-48 | Name and Transform as components | `Name` and `Transform` implement `Component`, so the schema can drive the inspector and serialisation (plan 5.10). Replaces D-18. | Confirmed by tests |
| D-49 | Parent as a tree link | Each object stores its parent link inside the ECS entry, next to its name and transform. The link is structure, not a value: the scene file stores the tree by nesting (WeaveLoom, L phase), so `Parent` has no schema. | Confirmed; file form decided in L1 |
| D-50 | Re-parenting | `Runtime::reparent` and `CommandBuffer::reparent` move an object and its subtree, keep its stable ID and loaded state, and refuse a sibling-name clash (`DuplicateName`), a cycle (`Cycle`), or a stale object or parent. Command failures are reported in `FrameReport::reparent_errors`. | Confirmed by tests |
| D-51 | Logging skipped commands | In debug builds, every skipped command is written to stderr with its command name and handle, as plan 5.8 requires. Stale handles have no path to print, so the handle is shown. | Confirmed |
| D-52 | Structural writes from stages | Scripts can insert, remove, and edit components, and attach or register-attach scripts, through `CommandBuffer`. These apply at the next boundary, and a rejected write is reported as `SkipReason::Rejected` or in `FrameReport::attach_errors`. Closes D-27 and D-35. | Confirmed by tests |
| D-53 | Reserved components | `Name`, `Transform`, and the internal ID and parent components cannot be inserted, removed, or edited through the generic component API. Changing a name or the tree goes through `reparent`, and a transform goes through `set_transform`. This keeps the sibling-name index consistent. | Confirmed by tests |

## Linux support and heart polish

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-54 | Linux support | Build, test, and clippy need only rustup, with no system libraries. The triangle needs a Vulkan driver, the loader, and a display (X11 or Wayland). Platform detection returns `Linux` on Linux, checked by a test. | Confirmed. The triangle ran for 6 seconds on Xvfb with Mesa's software Vulkan driver (lavapipe), and drew the expected gradient triangle. Screenshot in `docs/screenshots/linux-triangle.png`. Real GPU drivers not yet tested. |
| D-55 | One-command checks | `scripts/check.sh` (Linux and macOS) and `scripts/check.ps1` (Windows) run format, clippy, build, and test with `--locked`. The same steps run in CI. | Confirmed; the script runs clean on Linux |
| D-56 | Iteration order | `ObjectWorld` keeps its ID index sorted, so `objects()` yields objects in stable ID order, as the plan requires for every iteration that affects results. | Confirmed by test |
| D-57 | Diagnostics | `Runtime::describe` prints the object tree with each object's state and script count. `Runtime::stats` returns object, loaded, active, script, and queued-command counts. `FrameReport::is_clean` and `issue_count` summarise a frame. | Confirmed by tests |
| D-58 | Ease of use | `Runtime::update(dt)` runs a frame without a listener, and `NoListener` is exported for the same purpose. | Confirmed by test |
| D-59 | Linux smoke test | `scripts/linux-smoke.sh` builds the triangle, runs it on a virtual X display, checks it is still alive after 6 seconds, and saves a screenshot. Needs `xvfb` and `imagemagick`. | Confirmed on Linux |

## Derive macro

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-60 | Derive scope | `Component` covers named, tuple, and unit structs. Options: `name` on the struct, `rename` and `skip` on fields. Generics and enums are rejected with messages that point to `Field`. | Confirmed by tests and compile-fail examples |
| D-61 | Field derive | `Field` covers unit-variant enums, stored as variant names, and single-field structs, stored as their field. Generics are rejected. | Confirmed by tests |
| D-62 | Optional fields | `Option<T>` implements `Field`. A missing optional field loads as `None`, so files written before a field was added still load. A missing required field is still an error. | Confirmed by tests |
| D-63 | Numeric types | `i32` and `u32` are stored as integers with range checks. `f64` has its own kind, so it keeps full precision. | Confirmed by tests |
| D-64 | Syntax | `syn` is built with `clone-impls` in addition to `derive`, `parsing`, `printing`, and `proc-macro`. The lock file pins the version. | Confirmed by build |

## Heart review fixes

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-65 | Shared names | Each name is stored once as an `Arc<str>`, shared by the `Name` component and the sibling index. Reading a name clones a pointer and does not allocate. `path()` still builds a new string. | Confirmed by test (`Arc::ptr_eq`) |
| D-66 | Runtime state split | `Runtime` holds three parts: `Core` (world, commands, input, time), `Lifecycle` (loaded objects and pending work), and `Scripts` (attachments, registry, platform state). Each stage builds one environment from `Core`. Replaces eight copies of the same destructuring. | Confirmed by the existing suite (90 tests) |
| D-67 | Handle hashing | Maps keyed by `ObjectHandle` (attachments, child scopes, override set) use a small multiplicative hasher instead of the default SipHash. | Measured: see D-69 |
| D-68 | Logging | Skips, script failures, and rejected commands go through one sink set with `Runtime::set_log_sink`. The default writes to stderr in debug builds only. Nothing is printed from inside the runtime any more. | Confirmed by tests |
| D-69 | Benchmark | `examples/heart-bench` reports the best of seven runs per case, because single runs on this shared single-CPU machine varied by up to 60%. Current numbers, release build, 10,000 objects: plain 9 ns per object per frame, half scripted 19 ns, all scripted 38 ns. The earlier single-run baseline is too noisy to compare precisely, so no speedup is claimed. | Measured on this machine only |
| D-70 | Script helpers | `ScriptContext` acts on its own object directly: `set_transform`, `set_enabled`, `despawn`, `reparent`, `insert_component`, `edit_component`. These queue commands, so they take effect at the next frame boundary, as before. | Confirmed by tests |
| D-71 | Parent accessors | `parent_of(h)` returns the parent handle, `is_root(h)` returns `Option<bool>`, and both return `None` for stale handles. `parent(h)` is kept and documents the nested form. | Confirmed by tests |
| D-72 | Randomised tree test | `tests/model.rs` runs 40 seeded sequences of 300 random spawn, despawn, and re-parent operations against a reference model. After every step it checks the object count, id order, names, parents, paths, lookup by path, children, roots, and that stale handles stay dead. It also fails if a hard case never occurs (cycles, name clashes, stale targets). | Confirmed; 0 mismatches |
| D-73 | Services boilerplate | Superseded by D-74. | Superseded |

## Services without boilerplate

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-74 | Services as a trait parameter | `Script<S = ()>` takes the services type as a parameter with a default of `()`. A script with no services is `impl Script for X` and writes no declaration. A script that uses services declares them once, in the impl header: `impl Script<(Time, Scene)> for X`. Hooks take `ScriptContext<'_, S>`, and `ScriptContext` defaults to `()`. Undeclared use still fails to compile, for scripts with and without services. | Confirmed by tests and compile-fail examples |
| D-75 | Erasure | The runtime builds the erased dispatcher inside the generic `attach` and `register_script` calls, where the services type is known. The erased value is a generic adapter over the script and its services type, so no blanket impl over an unconstrained parameter is needed. Stored state is read back through the adapter. | Confirmed by tests |
| D-76 | Measured cost | The benchmark's all-scripted case ranged from 36 to 64 ns per object across identical runs on this virtual machine. The services change does not show a measurable difference, and no speed claim is made for it. | Measured; noise exceeds the effect |

## Benchmark documentation

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-77 | Every benchmark is logged | `docs/benchmarks.md` holds every benchmark run, with its command, environment, raw result, and what it does not show. A change that claims a speed effect must add an entry. Entries are never edited; a new run is a new entry that names the one it replaces. Superseded noisy runs stay in a history table, marked not comparable. | Confirmed |

## Renderer E3.1

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-78 | Math library | Vectors and matrices come from `glam` 0.34.3 (Rust 1.85, below the pinned 1.89). The heart still stores transforms as arrays, so math is not a project module yet. | Confirmed |
| D-79 | Projection convention | The camera uses `glam::camera::rh::proj::directx::perspective`, the DirectX and WebGPU convention: depth 0 to 1, Y up, which matches wgpu. | Confirmed by test (target centred, depth in range) |
| D-80 | Backend choice | DX12 on Windows, Vulkan on Linux, from the platform key. Other platforms use all backends. The choice is explicit because wgpu enables every backend by default. | Confirmed by tests |
| D-81 | Render graph | Passes declare what they read and write, and the graph rejects a read that no earlier pass wrote, a duplicate name, and a bad order. Each pass's CPU time is measured. `report()` prints the table. | Confirmed by tests |
| D-82 | Passes in E3.1 | `clear`, `opaque`, `transparent`, `post`. `transparent` and `post` record nothing yet; they exist so the graph has its final shape. | Confirmed |
| D-83 | Instance data | Model matrix and colour are per-instance vertex data, written once per frame into a buffer that grows by powers of two. One draw call covers all instances. This is the instancing that E3.3 will measure. | Confirmed by the cube example |
| D-84 | Lighting | Lambert shading from one directional light, with an ambient term and an exposure multiplier. No back-face culling yet (E3.4). | Confirmed by screenshot |
| D-85 | Renderable component | `Renderable { color }` is a heart component, derived with `weave_macros::Component`. `collect_instances` reads loaded, enabled objects that have a `Renderable` and a `Transform`. | Confirmed by tests |
| D-86 | Window ownership | The renderer takes any surface target. It does not depend on winit. The cube example owns the window, until `weave-platform` exists. | Confirmed |
| D-87 | Exit criterion status | Lit cube verified on Linux under Xvfb with lavapipe (screenshot and video). Windows not run. The loom report of passes is not built yet; the graph's own report is the stand-in. | Partly met |

## Renderer E3.2 to E3.4

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-88 | Shader hot reload | `use_shader_file` loads a file and builds from it. `reload_shader` is called once per frame and compares the file's modification time, so an unchanged file costs one metadata read. | Confirmed by B-005 (1.3 ms) |
| D-89 | Broken edits | A build runs inside a validation error scope. A failed build is reported as `ShaderError::Compile`, and the previous pipeline stays active. The error text is kept in `shader_error()` until the next good build. | Confirmed by B-005 |
| D-90 | Content-hash cache | Pipelines are cached by a 64-bit FNV-1a hash of the shader text, up to eight entries, evicting the oldest that is not active. Returning to a cached text switches with no recompile. | Confirmed by B-005 (`FromCache`) |
| D-91 | Batched and unbatched drawing | `DrawMode::Batched` is one instanced draw. `DrawMode::Unbatched` issues one draw per instance, with the same data. The second exists so the batching benefit can be measured against it. | Confirmed by B-004 (about 33 to 40 times less CPU recording) |
| D-92 | Frustum culling | Planes come from the view-projection matrix in the DirectX/WebGPU depth convention. Each instance is tested against its bounding sphere, with radius from the largest scale axis. Culling is off by default and turned on with `set_culling`. | Confirmed by unit tests and B-004 (5,068 of 10,000 kept) |
| D-93 | GPU timing gap | Frame times on this host (lavapipe, shared VM) vary by 10 to 15% between runs, which hides the frame-time effect of culling. GPU timestamp queries would measure the GPU work directly. Not built yet; the plan needs them for a stable E3.4 result. | Open |
| D-94 | Windows compile check | The whole workspace type-checks for `x86_64-pc-windows-msvc`. Nothing has been run on Windows. | Partly met |
| D-95 | Frame stats | `draw` returns `FrameStats` with the submitted count, the drawn count, and the CPU time of culling. `wait_idle` blocks until the GPU is done, for synchronised timing. | Confirmed |

## Renderer round: GPU timing, lights, resolution

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-96 | GPU timestamp queries | Each render pass writes a start and end timestamp, when the adapter supports `TIMESTAMP_QUERY`. Results are copied into a ring of three readback buffers and read once the GPU is done, so the frame never waits for them. Without support, the report says GPU timing is unsupported. Closes D-93 for the software device. | Confirmed on lavapipe (B-007 to B-009) |
| D-97 | Every pass begins a render pass | The placeholder `transparent` and `post` passes now begin empty render passes, so every pass always writes its timestamps. Without this, unwritten query slots would carry stale values. | Confirmed by tests and runs |
| D-98 | Lights | `Light` is directional, point, or spot. Up to 16 lights go into the frame uniform. Attenuation is inverse-square with a smooth cut-off at the range. Excess lights are dropped and counted in `FrameStats::lights_dropped`. Replaces `DirectionalLight`. | Confirmed by encoding tests and a screenshot |
| D-99 | Benchmark resolution | Benchmark entries state the window size. The renderer entries from B-007 on report GPU time. Media is captured at 1920x1080 for new entries; the earlier 800x600 media stays as history. `--size WxH` on the cube and benchmark binaries sets the window size. | Confirmed |
| D-100 | Culling result | At 1080p, culling halves the drawn instances in the dense scene but does not reduce GPU opaque time, because the cost is pixel fill. Culling is kept for scenes where much of the content is off-screen; a scene like that is still needed to show its benefit. | Measured (B-008) |
| D-101 | Remaining E3 items | Not built: LOD (E3.4 second half), PBR materials and compressed textures (E3.5), image-based lighting (E3.7), and a culling scene where content is mostly off-screen. Windows is type-checked only. | Open |

## Renderer round: complex geometry, lighting, and batches

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-102 | Light limit | 64 lights per frame. The frame uniform is 4,176 bytes, below the 64 KB default binding limit. | Confirmed by test |
| D-103 | Batches | `draw` takes a list of batches, each a mesh and its instances. One instance buffer holds every batch, and each batch records its own range. `add_mesh` uploads a mesh; `CUBE` is the built-in cube. | Confirmed by the showcase and the existing benchmarks |
| D-104 | Index width | Every mesh uses 32-bit indices. A complex mesh needs more than 65,536 vertices, and the cube keeps the same format for simplicity. | Confirmed by test |
| D-105 | Culling radius | Culling uses each mesh's bounding radius, scaled by the largest axis of the instance transform, rather than a fixed cube radius. | Confirmed by test |
| D-106 | Procedural meshes | `cube`, `ground`, and `lobed_sphere` are built in code. Normals of the lobed sphere come from numerical partial derivatives of the surface, and point outward. | Confirmed by test |
| D-107 | Light presets | `Light::key`, `Light::flood`, and `Light::accent` build the three roles the showcase uses. | Confirmed by test |
| D-108 | Plateau | GPU time stops rising above about 16 lights on lavapipe, in two scenes (B-010, B-011). Cause not determined. Needs a physical GPU. | Open |

## Renderer round: clustered lighting

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-109 | Clustered lighting | Lights are assigned on the CPU each frame to 16 by 9 screen tiles and 16 exponential depth slices. Each pixel shades only the lights in its own cluster. Replaces the per-pixel loop over every light. | Confirmed by tests and B-012 |
| D-110 | Light limits | Up to 4,096 point and spot lights in a storage buffer, plus 4 directional lights in the uniform. Excess lights are dropped and counted in `FrameStats`. The old 64-light limit is gone. | Confirmed by test |
| D-111 | Conservative assignment | A light is listed in every cluster that any point it reaches could fall in. Tile bounds account for the projected size of a sphere off the view axis. Lights wholly off-screen or behind the camera are dropped. The test samples points inside each light's range and fails if one is missing from its cluster. | Confirmed by tests |
| D-112 | Performance status | Small lights scale (B-012). Large lights and a fixed per-pixel overhead make the clustered path slower than the per-pixel loop at low counts, and at 64 big lights. Not resolved. Needs a physical GPU and probably a light-size-aware path. | Open |

## Renderer round: engine techniques for efficiency

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-113 | Exact cluster assignment | A light is listed in a cluster only if its sphere touches that cluster's frustum cell: the four side planes and the two depth planes. This follows the tile-and-cluster culling used in Forward+ renderers (Unity, Godot, Unreal) and is conservative. It halves the cost of large lights and cuts the top count by about a third (B-014). | Confirmed by tests (sampling test that fails on a missing light) and B-014 |
| D-114 | Direct path for few lights | Below a light-count threshold, the renderer uses a direct variant of the shader that loops over every light and skips the cluster build. The variant is generated from the clustered shader text, so both always update together. Threshold four, from B-013; the clustered path takes over above it. | Confirmed by B-013 |
| D-115 | Light path control | `Renderer::set_light_path` takes `Auto`, `Direct`, or `Clustered`. `Auto` is the default. The showcase exposes it as `--light-path`, so both paths can be measured on the same scene. | Confirmed |
| D-116 | Reload covers both variants | A shader edit rebuilds the direct variant as well. A broken edit in either variant keeps the previous pair. The reload scenario still passes (broken edit rejected, cache restore). | Confirmed by tests and the reload scenario |
| D-117 | Remaining gap to engine renderers | Not built: GPU-driven culling and indirect draws, per-cluster light lists built on the GPU, light-size-aware path selection beyond the count threshold, LOD, and clustered shadows. These are the next techniques from the same engines. | Open |

## Renderer round: directional shadows

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-118 | Directional shadows | The first directional light casts shadows. A depth-only pass renders the scene from the light into a 2048 by 2048 map, using an orthographic frustum of 44 units centred on the origin. The lit shader compares depth against the map with a comparison sampler. The pass is the first node in the render graph. | Confirmed by the showcase screenshot |
| D-119 | Shadow switch | `Renderer::set_shadows` turns shadows on or off (default on). Off means the shadow pass records an empty pass and the lit shader returns early on a uniform flag, so it costs nothing. | Confirmed by B-015 and B-016 |
| D-120 | Lookup cost | Four hardware-filtered comparison taps, not nine. The comparison sampler already filters neighbouring texels, so four taps give soft edges at lower cost. | Confirmed by B-016 |
| D-121 | Regression found by benchmark | The first shadow version computed the lookup for every pixel, even with shadows off, and was 30 ms slower in that case (B-016). Fixed by the early-out. Recorded so the cause is on record. | Fixed |
