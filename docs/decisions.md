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
| D-73 | Services boilerplate | `type Services = ();` cannot be removed on stable Rust. Associated type defaults are unstable, and removing the declaration would also remove the compile-time check that undeclared services fail. Kept as is. | Known limit |
