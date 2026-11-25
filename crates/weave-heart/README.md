# weave-heart

Owner area: core. Boundary: objects, their hierarchy, stable IDs, transforms, component storage and schema, the frame lifecycle, the command buffer, scripts, script services, and platform overrides. Its only runtime dependency is `hecs`.

Phase H1: object IDs, handles, parent and child objects, stable paths, sibling-name uniqueness, transforms, stale-handle safety.

Phase H2: the `Component` trait, the `Field` value layer, and compile-time schema generation. Derive with `weave_macros::Component`. Components attach to objects through `ObjectWorld` or `Runtime::insert_component`.

Phase H3: `Runtime` drives the seven stages from plan 5.7 each frame. `CommandBuffer` carries structural changes and lifecycle requests, applied at the frame boundary. `FrameReport` returns skipped commands, spawn errors, script failures, and override errors. The `Listener` trait observes each stage.

Phase H4: scripts. `Script` has a hook per stage. Attach an instance with `Runtime::attach`, or register a named modular script with `register_script` and attach it with `attach_modular`. Each attachment keeps its own state, read back with `Runtime::script::<T>`.

Phase H5: service context. Each script declares its services in `type Services`. `services` provides `Time`, `Input`, and `Scene`, and undeclared use fails to compile. A panicking script is disabled in debug builds and reported; release builds let the panic propagate.

Phase H6: platform overrides. `PlatformOverride` holds per-platform script lists and applies the current platform's scripts when an object first loads. `keep_only` supports build-time stripping. Component value overrides are not yet built (decision D-46).

Not yet here: platform overrides for component values (L1), scripts attaching from inside a stage (D-35), and the editor-facing parts of scripts (W phases).
