# weave-heart

Owner area: core. Boundary: objects, their tree, stable IDs, the core components (`Name`, `Transform`), component storage and schema, the frame lifecycle, the command buffer, scripts, script services, and platform overrides. Its only runtime dependency is `hecs`.

- H1: object IDs, handles, child objects, stable paths, sibling-name uniqueness, stale-handle safety.
- H2: the `Component` trait, the field value layer, and compile-time schemas through `weave_macros::Component`. `Name` and `Transform` are components.
- H3: `Runtime::tick` runs the seven stages from plan 5.7 each frame. `CommandBuffer` carries structural changes and lifecycle requests, applied at the frame boundary. `FrameReport` returns skipped commands, spawn, reparent, attach, script, and override errors. Skipped commands are logged in debug builds.
- H4: scripts. Each attachment keeps its own state. `Runtime::script::<T>` reads it back.
- H5: services are declared per script (`Time`, `Input`, `Scene`), and undeclared use fails to compile. A panicking script is disabled in debug builds and reported; release builds let the panic propagate.
- H6: `PlatformOverride` attaches the current platform's scripts once, when an object first loads. `keep_only` supports build-time stripping.
- Completion pass: re-parenting with name-clash and cycle checks; component edits, removals, and script attachment from inside stages; reserved components protected from the generic API.

Not in this crate: component value overrides per platform (decision D-46, phase L1), the scene and object file formats (phase L), and the editor (phase W).
