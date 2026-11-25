# Weave Engine: Master Plan

Revision 6. This document replaces Revision 5. It holds the scope, architecture, subsystems, roadmap, risks, and decision records, plus the research evidence in Appendix B. Changes from Revision 5 are listed in Appendix C.

Owner: hobby engine, public and open source (dual MIT or Apache-2.0). Windows first, Linux second. Android and iOS reserved. Alpha 1 is the first public release. The shooter is a stress test that drives engine improvements, and it is built as a module after the engine is stable.

Contents
- Part I: Plan (Sections 0 to 21)
- Part II: Decision records (Appendix A)
- Part III: Research evidence (Appendix B) and changes from Revision 5 (Appendix C)

---

# Part I: Plan

## 0. How to read this document

Every factual claim carries a tag:
- **[V]** verified from an official or primary source (Appendix B), checked on the research date in Appendix B.1.
- **[S]** secondary or self-reported.
- **[X]** not verified. A named phase checks it before the claim is relied on.
- **[DESIGN]** a proposal in this plan.
- **[TARGET]** a goal that a benchmark must confirm before it counts as achieved.
- **[OWNER]** a rule the owner stated and confirmed.
- **[D-xx]** points to a decision in Appendix A.

Version numbers are never fixed in the plan unless a phase has pinned them. Every crate is pinned at the phase that first uses it, to the latest release at that time, and the pinned version is recorded in the decision record.

No performance figure is a measured result until its benchmark exists and its result is committed.

## 1. Vision and goals

### 1.1 Vision
A Rust-native 3D engine with an object and component model defined by its owner. It is fast by design, usable entirely from the command line, and has an optional editor. A shooter proves it.

### 1.2 Goals
- **G1. Rust-native.** No native code in the engine core unless a decision in Appendix A justifies it.
- **G2. Pay only for what you use.** Unused subsystems and features are not in the shipped build (Section 12).
- **G3. One object, component, and script model** (Section 4).
- **G4. Editor optional.** A project can be created, built, run, and packaged entirely from the command line. The editor is a separate program with CLI parity (Section 15).
- **G5. Fast by measurement.** Every subsystem has a budget and a benchmark (Section 14).
- **G6. Full documentation.** Every public API has a reference page and an example.
- **G7. The shooter is the final test.** It is built as a module on top of the engine.
- **G8. Deterministic when asked.** A deterministic build profile exists, with a stated scope (Section 5.4).

### 1.3 Non-goals for engine alpha 1
- Hot reload of Rust game logic and modules (deferred; Section 13).
- Hot reload of map scenes (deferred; Section 13).
- Built-in networking.
- Mobile platforms.
- Non-Rust script authoring. Shaders are the exception and use WGSL.
- Skybox rendering (deferred to a later release; Section 7.4).

---

## 2. Scope: what each release contains

| Area | Alpha 1 | Alpha 2 | Later |
|---|---|---|---|
| Objects, child objects, components | Yes | | |
| mainScript, scripts, modularScripts, Scripts base object | Yes | | |
| Base scripts (system modularScripts), Random included | Yes | | |
| Services and capabilities (compile-time typed context) | Yes | | |
| Plugin API (D-17) | Yes | | |
| Scene format (text, versioned, content-hashed) | Yes | | |
| CLI: new, build, run, check, scene, package, report | Yes | | |
| Editor (gpui), optional, with CLI parity | Yes | | |
| Rendering core, WGSL shaders, basic PBR | Yes | | |
| Shader and asset hot reload | Yes | | |
| Cubemap skybox | | | Later |
| Image-based lighting (environment map, prefiltered) | Yes | | |
| Lighting: directional, point, and spot lights, ambient | Yes (unshadowed) | | |
| Lighting: light emitter component (one component, light kind field) | Yes | | |
| Shadows | | Yes | |
| Lighting baking (`engine bake`) | | Yes | |
| Physics core (Rapier backend, shapes, bodies, joints) | Yes | | |
| Ragdoll (handoff, PD drive or spring drive, recovery by blend) | Yes | | |
| Basic skeletal animation (clips, blending) | Yes | | |
| Audio core (`cpal` and `kira`), 3D sound, mock backend for tests | Yes | | |
| Input (action mapping) | Yes | | |
| Camera (follow, first-person) | Yes | | |
| Storage core (platforms, data types, atomic writes, service) | Yes | | |
| Two-bone IK and foot placement | | Yes | |
| Storage encryption (`storage-crypto`) | | Yes | |
| In-game UI | | Yes | |
| Audio occlusion | | Yes | |
| Navigation: navmesh (`rerecast`) and A* pathfinding | | Yes | |
| Module ABI and SDK | | Yes | |
| Procedural sky and time of day | | | Later |
| Map generation and streaming | | | Later |
| Navmesh crowds | | | Later |
| Particles | | | Later |
| Volumetric smoke | | | Later |
| Hot reload of Rust game logic and modules | | | Later |
| Hot reload of map scenes | | | Later |
| WebAssembly sandboxing | | | Later |
| Motion matching, meshlets, GPU-driven culling | | | Later |
| Networking | | | Later |
| Android, iOS | | | Later |

Notes:
- Ragdoll is in alpha 1 at the owner's request. It needs basic animation, which is included. IK and foot placement are in alpha 2.
- Alpha 1 lighting is unshadowed, and includes image-based lighting so PBR materials get real ambient reflections without a visible sky.
- The module ABI is in alpha 2. Built-in subsystems use the plugin API (D-17) from alpha 1.
- Alpha 1 is larger than a typical first release. Section 20 records this as a risk, and Section 19 defines a checkpoint.

---

## 3. Architecture

### 3.1 Layers

```
+----------------------------------------------------------------+
|  Project: objects, components, scripts, scenes, assets         |
+----------------------------------------------------------------+
|  Editor (optional, separate program)  |  Modules (alpha 2+)    |
+----------------------------------------------------------------+
|  Base scripts in the Scripts base object (engine-owned)        |
+----------------------------------------------------------------+
|  Subsystems and leaf features (optional)                       |
|  physics | ragdoll | render | lighting | anim | audio | ...   |
+----------------------------------------------------------------+
|  Core runtime: objects, components, scripts, scheduler,        |
|  registry, services, command buffer                            |
+----------------------------------------------------------------+
|  Core services: math, jobs, logging, profiling, IO             |
+----------------------------------------------------------------+
|  Platform: windowing, input events, filesystem, threads        |
+----------------------------------------------------------------+
```

### 3.2 Workspace

```
engine/
  crates/
    core/            math, jobs, logging, profiling, io, ids           (alpha 1)
    platform/        windowing, input events, filesystem, threads      (alpha 1)
    runtime/         objects, components, scripts, registry, services,
                     scheduler, command buffer                         (alpha 1)
    engine-macros/   derive macros for components, script schemas      (alpha 1)
    plugin-api/      the one plugin interface (D-17)                   (alpha 1)
    scene/           scene graph, text format, content hashes          (alpha 1)
    render/          wgpu renderer, render graph, PBR, IBL             (alpha 1)
    lighting/        lights, light emitters, shadows, baking           (core alpha 1; shadows and baking alpha 2)
    shaders/         WGSL pipeline, cache, hot reload                  (alpha 1)
    physics/         PhysicsBackend, Rapier backend, character         (alpha 1)
    ragdoll/         ragdoll leaf of physics                           (alpha 1)
    anim/            skeletons, clips, blending                        (alpha 1; IK alpha 2)
    audio/           AudioBackend, Kira backend, mock backend, 3D      (alpha 1; occlusion alpha 2)
    input/           action mapping                                    (alpha 1)
    camera/          camera rigs                                       (alpha 1)
    storage/         storage core                                      (alpha 1)
    storage-crypto/  encryption leaf                                   (alpha 2)
    prebuilt/        Scripts base object and base scripts              (alpha 1)
    assets/          import, cache, packaging, asset hot reload        (alpha 1)
    devtools/        console, profiler data, debug draw                (alpha 1)
    ui/              in-game UI                                        (alpha 2)
    nav/             navmesh (rerecast), pathfinding                   (alpha 2)
    modules-api/     module ABI (D-16)                                 (alpha 2)
    mapgen/          noise, chunks, streaming                          (later)
    vfx/             particles, smoke                                  (later)
  tools/
    cli/             the `engine` command                              (alpha 1)
    editor/          editor, built on gpui                             (alpha 1)
examples/
docs/
benches/
```

### 3.3 Dependency rules
- `core` depends on nothing in the engine.
- Subsystems depend on `core`, `runtime`, and `plugin-api`. Cross-subsystem use goes through signals or documented traits.
- A leaf depends on its parent core, never the reverse. `storage-crypto` depends on `storage`. `ragdoll` depends on `physics` and `anim`.
- The editor depends on the engine's public API and the scene format. The engine never depends on the editor.
- `devtools` is never a dependency of a subsystem.
- `engine-macros` is a proc-macro crate with no engine dependencies.
- CI rejects cycles, and CI builds the engine with no editor present.

---

## 4. The object, component, and script model

### 4.1 Concepts

| Concept | What it is | Who can use it |
|---|---|---|
| **Object** | A named node in the scene hierarchy. It has a name, a parent, a transform, child objects, and components. In the runtime it is a hecs entity (Section 4.9). | Lives in a scene or in a template |
| **Child object** | An object inside another object. Moves with its parent. | Its parent and the project |
| **Component** | Plain data attached to an object. Holds no per-frame behaviour (Section 4.12). | Any system that declares it |
| **Scripts component** | Lists the script instances attached to an object (Section 4.12). | The object |
| **mainScript** | Runs when its object loads. Optional. Default is empty. | Only its own object |
| **scripts** | Behaviour attached to one object. Any number. | Only the object they belong to |
| **modularScripts** | Reusable script definitions in the Scripts base object. Rust types registered at compile time (Section 4.8). | Any object can attach them |
| **Base scripts** | System modularScripts that ship with the engine (Random is the first). | Any project |
| **Reusable object** | A template. Instances copy it and can override fields. | Instanced into scenes |
| **Service** | An engine capability (storage, input, audio, random source) reached through a typed context (Section 4.8). | Scripts that declare it |

### 4.2 Child objects
- An object can contain child objects to any depth.
- A child's transform is relative to its parent.
- Physics on children [OWNER-confirmed]:
  - A child with a collider and no body of its own contributes to its parent's body as a compound shape.
  - A child that declares its own body is a separate physics body, connected to its parent by a joint if the owner declares one. Ragdoll uses this form.
- Removing a parent removes its children, in child-first order (Section 4.4).

### 4.3 The Scripts base object
- Ships with the engine and is included in every new project.
- Holds all modularScripts available to the project: the engine's base scripts and the project's own.
- Visible to `engine report` and to the editor.

### 4.4 Lifecycle
Stages run in this order each frame. Within a stage, objects run in stable path order (Section 4.13).

1. `mainScript.on_load`: once, when the object loads. Default: nothing.
2. `on_enable`: when the object or its component becomes active. Also runs after `on_load`.
3. `on_ready`: once, after the scene loads, after all `on_enable` calls.
4. `on_fixed_update(dt)`: zero or more times per frame, at the physics timestep.
5. `on_update(dt)`: once per frame.
6. `on_late_update(dt)`: once per frame.
7. `on_signal(sig)`: when a subscribed signal fires. Signals sent during a stage are delivered at the start of the next stage (Section 4.6).
8. `on_disable`: when the object or component is deactivated.
9. `on_unload`: once, on removal. Child objects unload before their parent.

Structural changes requested inside a stage (adding or removing objects) are queued in the command buffer and applied at the frame boundary (Section 4.13).

### 4.5 Identity
- Objects are identified by a 64-bit generational handle. The handle is the hecs `Entity` if its layout permits the required generation check, or a wrapper with its own generation otherwise. The choice is made in Phase 0.3 after reading the hecs layout. [DESIGN, B2]
- A stale handle returns "none" and never crashes.
- Objects have a stable path (for example `world/player/arm_left`), used in scene files and diagnostics. It is not a runtime key.
- Sibling names are unique. A duplicate name in a scene is a `engine check` error, and the editor refuses it. [DESIGN]
- Each script type and component type registers a stable name and a schema hash. Saved data uses these, never Rust's internal type IDs (Section 4.9).

### 4.6 Signals versus direct references [OWNER-confirmed, D-23]
- **Signals** when the sender does not know its receivers, or several objects may react: "player died", "door opened".
- **Direct references** (a stored object handle) when one object owns or depends on a specific other: a turret holding its target.
- Direct references are checked on use. Stale handles return "none".
- Signals carry typed payloads. Signals are queued and delivered at the start of the next stage, so delivery order is deterministic. [DESIGN]
- Review rule: no direct reference between objects without a clear owner relationship, and no signal that carries data only one receiver uses.

### 4.7 Script state and reusable objects [OWNER-confirmed, D-24]
- Script state is per object instance. A modularScript on ten objects has ten states.
- Overriding a field on an instance changes that instance only.
- Template edits propagate unless the instance overrides the field.
- State survives a template edit unless the script's schema changes. A schema change needs a migration or resets the state with a warning.
- Field overrides use the reflection schema generated by the derive macro (Section 4.9).

### 4.8 Services, capabilities, and script failure [OWNER-confirmed, D-18]
- Scripts declare the services they need in their type. The engine passes a context value to each script callback that contains only the declared services. A script that touches an undeclared service does not compile. [DESIGN, approved]
- Storage is a service, not an object or script type. The base project has no Storage object.
- modularScripts are Rust types, registered at compile time through the plugin API. The Scripts base object lists the registered types. [DESIGN, approved]
- Script failure: in development builds, a panic in a script disables that script instance, logs the error with the object path, and continues. In release builds, a script panic stops the game. [DESIGN, approved]

### 4.9 Registry and identity inside the engine
- Components are stored in hecs archetypes (D-11). Rust's `TypeId` is used only within a running process.
- Each component and script type derives a stable name and a schema hash through `engine-macros`. The derive also generates a field schema for overrides, the inspector, and serialization. Rust has no built-in reflection, so the derive is the reflection mechanism. [DESIGN, approved]
- Saved data, scene files, and the CLI use stable names and schema hashes only.
- Scene files store components by stable name and schema hash. [DESIGN, approved]

### 4.10 Scheduling
- Systems declare reads and writes on component types. Non-conflicting systems run in parallel.
- Ordering comes from explicit dependencies, and within a stage from stable path order.
- Frame order: input, fixed-step physics, script update, animation, audio update, late update, command buffer flush, render submission, present.
- Phase 0.7 compares the custom scheduler with `bevy_ecs`'s scheduler on one workload before Phase 2.8 commits to the design. [DESIGN, approved]

### 4.11 Base project
A new project contains:
- **World:** scene root. Gravity, time scale, environment (ambient and image-based lighting settings).
- **Player:** controller, camera target, input mapping.
- **Camera:** camera rig and active view, with its `AudioListener`.
- **Scripts base object:** the modularScripts library, with base scripts included.

### 4.12 Components

**Definition.** A component is a `#[derive(Component)]` struct from `engine-macros`. It holds data only. Behaviour lives in scripts and systems. [DESIGN, approved]

**Attachment.** An object's components are stored on its hecs entity. Script instances are listed in the `Scripts` component. [DESIGN, approved]

**Required components.** Every object has `Name`, `Transform`, and `Parent`. Non-object entities are not allowed in scenes. [DESIGN, approved]

**Catalogue.** Alpha tags show where each component first ships. Rows marked **gap** were missing from Revision 5.

| Component | Group | Alpha | Notes |
|---|---|---|---|
| `Name`, `Parent`, `Children` | Hierarchy | 1 | Required (Section 4.2) |
| `Transform` | Core | 1 | Local value, cached world value, dirty flag. Parent changes mark children dirty [general, verify] |
| `Tags` / `Layer` | Core | 1 | Filtering for physics, rendering, and queries. **Gap** |
| `Visibility` | Core | 1 | Visible flag and culling mode. **Gap** |
| `Scripts` | Behaviour | 1 | List of script instances on the object. **Gap** |
| `Camera` | Rendering | 1 | Projection, FOV, near and far planes, viewport, clear mode. **Gap** |
| `MeshRenderer` | Rendering | 1 | Mesh and material handles. Shadow flags reserved for alpha 2 |
| `SkinnedMeshRenderer` | Rendering | 1 | Animated characters (Section 8.3). **Gap** |
| `Environment` | Environment | 1 | Ambient colour and intensity, environment map for image-based lighting. Replaces the single constant ambient term. **Gap** |
| `Skybox` | Environment | Later | Deferred (Section 7.4) |
| `LightEmitter` | Lighting | 1 | One component with a `kind` field: directional, point, or spot. Replaces the separate light concept of Revision 5. [DESIGN, approved] |
| `RigidBody` | Physics | 1 | Type, mass, damping, sleep settings |
| `Collider` | Physics | 1 | Shape, material, sensor flag, collision groups |
| `Joint` | Physics | 1 | Attaches a body to another |
| `CharacterController` | Physics | 1 | Kinematic capsule (Section 8.1) |
| `Trigger` | Physics | 1 | Sensor events delivered as signals. **Gap** |
| `Ragdoll` | Physics | 1 | Leaf of physics (Section 8.2) |
| `Animator` | Animation | 1 | Clip set and state machine |
| `IKTarget` | Animation | 2 | Two-bone IK and foot placement |
| `AudioSource` | Audio | 1 | Clip, volume, 3D range, loop |
| `AudioListener` | Audio | 1 | Usually on the camera. **Gap** |
| `NavAgent` | Navigation | 2 | Pathfinding (Section 3.2 `nav/`) |
| `Fog` / `Atmosphere` | Environment | Later | Deferred |

Game-specific components, such as health or weapons, belong in the shooter module, not the engine.

### 4.13 Frame structure and command buffer [DESIGN, approved]
- Structural changes (creating or destroying objects, adding or removing components, changing parents) requested during a stage are written to a command buffer. The buffer is applied at the command buffer flush point in Section 4.10.
- Commands apply in the order they were queued. Within a stage, objects are visited in stable path order, so the result is the same on every run.
- A stale handle in a queued command is skipped and logged in development builds.

---

## 5. Engineering rules

### 5.1 Code style [OWNER-confirmed, D-15, revised by B3]
- No comments in source files, with one exception: `// SAFETY:` comments on `unsafe` blocks, which Clippy's `undocumented_unsafe_blocks` lint expects. [DESIGN, approved]
- API documentation lives in `docs/api/` and is included at each crate root with `#![doc = include_str!(...)]`. This is an attribute, not a comment.
- `rustfmt` enforced. `clippy` denies correctness lints.

### 5.2 Safety
- `unsafe` is allowed only in: GPU buffer mapping (render), memory utilities (core), audio callback boundaries (audio), and the module boundary (alpha 2).
- Every `unsafe` block has a `// SAFETY:` comment and an invariant entry in `docs/safety/`. CI fails if the count rises without a review entry.

### 5.3 Errors
Library crates return typed errors. No `unwrap` or `expect` in library code outside tests.

### 5.4 Determinism [revised]
- **Deterministic profile.** Rapier's `enhanced-determinism` feature is used in the deterministic profile. Rapier's own source refuses to compile when SIMD and `enhanced-determinism` are both enabled [V, rapier2d and rapier3d source]. The deterministic profile therefore builds without SIMD. [DESIGN]
- **Parallel solving under determinism is untested.** The source does not show a compile error for `parallel` with `enhanced-determinism`, but published asset variants exclude `parallel` from their deterministic builds. Phase 0.4 settles this on the pinned version. [X]
- **Seeded generators.** Simulation uses one named, seeded PRNG per session. Pick a generator whose output is documented as reproducible: `rand_chacha` or `rand_xoshiro` from the rand ecosystem [V, rand docs list both]. `rand::rng()` (the thread-local generator) is not used in simulation. Reproducibility is per algorithm, not guaranteed across all rand generators [V, rand book]. [DESIGN, approved]
- Simulation reads only the fixed-step clock.
- Per-platform replay is required. Cross-platform determinism only where the physics backend supports it on the pinned version (Phase 0.4).

---

## 6. Core services (alpha 1)

**Math.** Vectors, matrices, quaternions, transforms, AABB, OBB, frustum, rays, capsules. Benchmark a standard SIMD-friendly math crate against a minimal custom set in Phase 1.2. [X] The math type chosen must match the physics backend's math types. [X]

**Jobs.** A job system benchmarked against `rayon` in Phase 1.3. Rapier exposes `rayon` through its `parallel` feature [V]. [X] on whether the job system and Rapier share a thread pool.

**Logging and profiling.** Structured logs with levels. Profiler zones are recorded per frame and feed the devtools console (Section 17). [DESIGN]

**IO.** Platform file access through one trait, so the storage core and the asset cache use the same path.

**Random.** Section 5.4 governs simulation randomness. Non-simulation randomness (cosmetic effects) may use `rand::rng()`. [DESIGN]

---

## 7. Rendering and lighting (alpha 1)

### 7.1 Renderer core
- Backend: `wgpu` [D-02]. `wgpu` 29.0.1 is the latest release seen in this research pass [V, docs.rs]. The renderer pins the latest release at Phase 4.1 and records it in Appendix A. The earlier figure of 30.0.1 is not confirmed. [X]
- Backend selection on Windows: `wgpu`'s default enables all backends [V]. The engine selects DX12 on Windows with Vulkan as fallback, and Vulkan on Linux [DESIGN]. The choice is configurable through an engine setting and through `WGPU_BACKEND`.
- Store operations use `StoreOp::Store` and `StoreOp::Discard`. The old `store: bool` form is not used in any example [V, wgpu changelog].
- Render graph with passes for shadows (alpha 2), opaque, transparent, and post. [DESIGN]
- PBR materials with metallic-roughness workflow. [X on exact model]

### 7.2 Shaders
- WGSL only [D-03, D-14]. Naga translates WGSL to each backend [V].
- Hot reload per Section 13. [DESIGN]

### 7.3 Lighting

**Units [approved].** Physical units with an exposure setting:
- Directional light: illuminance in lux.
- Point and spot lights: luminous intensity in candela, with a range in metres.
- The `Environment` component holds an exposure value in EV. Auto-exposure is later. [DESIGN]

**Kinds.** `LightEmitter` has a `kind` field: directional, point, or spot. [approved]

**Alpha 1.**
- Directional, point, and spot lights, unshadowed.
- Ambient from the `Environment` component.
- Image-based lighting: diffuse irradiance and specular prefiltered maps generated from one environment map at load time. Uses the split-sum approximation [general, verify in Phase 5.5]. [approved]
- Light emitter objects, including as children.

**Alpha 2.**
- Shadow maps: cascaded for directional lights; separate leaves for point and spot lights.
- Baking: an offline step, `engine bake`, writes lightmaps as assets. Never run at runtime.

**Interface for later systems.** Lighting exposes light transmittance queries. Smoke (later) and vision checks will use them, so the interface is defined in alpha 1.

**Target [TARGET].** 500 point lights within the GPU budget (Phase 5.2 benchmark).

### 7.4 Skybox (deferred) [owner decision]
- No skybox in alpha 1. The `Skybox` component is reserved in Section 4.12.
- When it is scheduled, the plan will choose between a cubemap skybox (from `.hdr` or `.exr`) and a procedural sky driven by the directional light.
- Alpha 1 IBL uses the environment map's data but does not draw it as a backdrop.

---

## 8. Physics, ragdoll, and animation (alpha 1)

### 8.1 Physics core
- Backend: Rapier, pure Rust, behind `PhysicsBackend` (D-04). The pinned version is recorded at Phase 0.4. The latest `rapier3d` release seen in this research pass is 0.34.0 [V, docs.rs]. The earlier figure of 0.35.3 is not confirmed. [X]
- Shapes: sphere, box, capsule, cylinder, convex hull, static triangle mesh, heightfield.
- Bodies: static, kinematic, dynamic, sensor.
- Joints: fixed, hinge, ball-socket, slider, distance, 6-DOF, with limits and motors. Cone-twist: [X].
- Customisation: per body (mass, inertia, friction, restitution, damping, gravity scale, sleep thresholds), per world (timestep, solver iterations, skin width), per material (friction model, restitution combine rule), per joint (limits, motors, stiffness, damping, break force). Contact-modification and collision-filter hooks.
- Fixed timestep:
  ```
  accumulator += frame_dt
  while accumulator >= dt_phys:
      step(dt_phys)
      accumulator -= dt_phys
  alpha = accumulator / dt_phys
  render_pos = (1 - alpha) * prev_pos + alpha * curr_pos
  ```
  `dt_phys` = 1/120 s default (D-08). At most 8 steps per frame. [DESIGN]
- Contact impulse, normal direction (standard):
  ```
  j = -(1 + e) * (v_rel . n) / (1/m_a + 1/m_b + n . ((I_a^-1 (r_a x n)) x r_a + (I_b^-1 (r_b x n)) x r_b))
  ```
- Continuous collision (alpha 1 leaf): sweep along `d = v * dt` and clamp at the time of impact `t_hit` in [0, 1].
- Character controller: kinematic capsule on Rapier's shape queries [X].
- Trace recorder: contacts and solver impulses to a ring buffer.

**Targets [TARGET].** 1,000 dynamic bodies with about 5,000 contacts: physics at most 3 ms per frame. Character update: at most 0.05 ms.

### 8.2 Ragdoll (alpha 1)
**Definition.** A character is an object whose children are body parts (head, torso, arms, legs), each with its own body, connected by joints. This uses the child-object model (Section 4.2).

**Multibody joints [V, Rapier docs and loader sources].** Rapier multibodies are tree-structured. A ragdoll is a tree, so it fits. A loop, such as a four-bar linkage or a rod closing two limbs, must use ordinary impulse joints. Multibody joints support implicit springs and per-DoF damping [V, Rapier test source]. Loops are not part of the ragdoll design unless a character needs one.

**Mapping.** A high-detail animation skeleton drives a low-detail physics skeleton, and back. Jolt documents the same idea [V]. [DESIGN]

**Handoff with velocity matching.**
```
v_bone = v_root + omega_root x (r_bone - r_root)
q_delta = q_t * conj(q_(t - dt))
omega ≈ 2 * vec(q_delta) / dt        (sign chosen so w >= 0)
```
Each body's linear and angular velocity is set to the animated values before dynamics is enabled.

**Driven pose.** Two options, decided at Phase 0.5:
1. Rapier multibody springs and per-DoF damping, which are built into the solver. [V]
2. A custom PD controller:
   ```
   tau = k * (theta_target - theta) - c * omega
   ```
   Per joint `k` and `c`, so arms can be softer than the spine.
The first option is tried first. The second is used only where it fails. [approved]

**Blend with smoothstep.**
```
s = clamp(t / T, 0, 1)
w = 3 s^2 - 2 s^3
pose = (1 - w) * anim_pose + w * physics_pose
```

**Recovery (alpha 1).** When the body rests near a valid pose, blend back to animation with the same formula. Foot placement is alpha 2, so alpha 1 blends to a stored rest pose and checks for ground contact.

**Requirement [V, Phase 0.5].** A tree ragdoll uses multibody joints. A ragdoll that needs a loop uses impulse joints, which must be estimated before the ragdoll phase is scheduled.

**Targets [TARGET].** 160 ragdolls in a pile, mostly sleeping, at most 2 ms. Handoff at most 0.02 ms each.

### 8.3 Animation (alpha 1, basic)
- Skeletal clips with quaternion slerp for rotations and linear interpolation for translations.
- Blending between clips by weight.
- A simple state machine with conditions.
- Hooks for ragdoll handoff.
- Not in alpha 1: two-bone IK and foot placement (alpha 2), motion matching (later).

**Target [TARGET].** 200 animated characters with blending at most 2 ms.

---

## 9. Audio (alpha 1)

**Backend.** `cpal` for output and `kira` for mixing, tweens, tracks, effects, and spatial audio, behind `AudioBackend` (D-13). The latest `kira` release seen in this research pass is 0.12.0 [V, docs.rs]. `kira` enables `cpal` by default and exposes device and stream configuration in its settings [V].

**Testing.**
- A mock backend ships with `kira` [V]. CI runs the audio tests on the mock backend, so tests run without an audio device.
- The `assert_no_alloc` feature panics if memory is allocated on the audio thread. The engine enables it in debug and CI builds [V, feature exists; default-on behaviour in debug to verify]. [X]
- Linux testing is a Phase 0.6 and 8.1 exit criterion, since `kira` is mainly tested on Windows [V].

**Scope.**
- Mixing with tracks and volume control.
- 3D positional sound with distance attenuation and panning.
- Streaming for long tracks.
- Audio runs on its own thread with a lock-free command queue. [DESIGN]
- Audio is a service (Section 4.8). Scripts declare it.
- Output device selection is a settings value, defaulting to the system default device. [DESIGN]

**Not in alpha 1:** occlusion (alpha 2).

**Target [TARGET].** 128 simultaneous positioned voices in at most 0.5 ms per audio tick.

---

## 10. Input, camera, and scene basics (alpha 1)

**Input.** Action mapping with button, axis, and vector2 types. Rebinding. Combined bindings. Dead zones. Applied in the frame it arrives.

**Camera.** Follow and first-person rigs. Blends use the smoothstep formula in Section 8.2. The camera component is defined in Section 4.12.

**Scene graph.** Hierarchy of objects with local transforms and dirty-flag propagation, so cost scales with changes, not with scene size. Fyrox documented this approach in 0.36 [V].

---

## 11. Storage (alpha 1 core)

### 11.1 Storage core
- Multi-platform file handling for the supported platforms.
- Data types: normal records, typed fields, schema versions, migrations.
- Atomic writes and an index.
- Typed tiers: `Normal<T>` and `Secure<T>`. In alpha 1, `Secure<T>` is a marker type. A write of `Secure<T>` to a `Normal` field fails at compile time, and a write of `Secure<T>` to disk fails with a clear error until the encryption leaf exists in alpha 2. [DESIGN, B1 approved]
- Storage access from scripts through the storage service (Section 4.8).

### 11.2 Storage encryption leaf (alpha 2)
- AEAD: AES-256-GCM where AES hardware is present, ChaCha20-Poly1305 otherwise. Algorithm ID in header (D-07).
- Sealed key derivation: Argon2id, RFC 9106 memory-constrained profile by default (m = 64 MiB, t = 3, p = 4). High-security profile (m = 2 GiB, t = 1, p = 4) opt-in on desktop [V, RFC 9106].
- Threat model: protected data resists casual editing and tampering. The key ships with the game, so a determined attacker can recover it. Not for anti-cheat, accounts, or payments.

### 11.3 File format [DESIGN]
```
header:  magic(4) | format_version(2) | algorithm_id(1) | flags(1) | schema_hash(8)
index:   count(4) | per-record: id(8) | tier(1) | offset(8) | length(4)
records: payload in 64 KiB chunks
```
Encrypted chunks: `nonce(12) | ciphertext | tag(16)`, about 0.04 percent overhead at 64 KiB. Nonces use a per-file random prefix plus a per-chunk counter. [X: AEAD sizes, Phase 11.3 in alpha 2]

**Target [TARGET].** Normal records: 1 MB read in at most 1 ms from a warm cache.

---

## 12. Packaging: pay only for what you use (D-20)

### 12.1 Rule
A release package contains only the subsystems, leaf features, base scripts, and script types the project uses.

### 12.2 Mechanism
- Every leaf is a Cargo feature. Projects enable only what they need.
- Release builds link statically. Unused code is removed by dead-code elimination and link-time optimisation.
- The plugin registry is built at compile time from enabled features.

### 12.3 Constraints
Dead-code removal only works if nothing keeps code alive:
- Dynamic loading keeps whole libraries. It is excluded from release builds. Alpha 1 has no dynamic loading of game code.
- Reflection-style registries must register only enabled types. The derive macro registers only the types compiled into the project.
- Leaf features must be independent. CI builds a minimal project per leaf and fails if unexpected code appears.

### 12.4 Package report
Every release writes a report: enabled subsystems, leaves, base scripts, script types, component types, and binary size per subsystem.

---

## 13. Hot reload

### 13.1 In alpha 1
- **Shaders:** reload and swap (Section 7.2).
- **Assets:** textures, meshes, materials, audio clips, and animation clips reload in place. Handles stay valid. A failed reload keeps the previous version and reports the error. Dependent assets reload in dependency order.
- **Target [TARGET].** Texture reload at most 100 ms. Shader reload at most 300 ms.

### 13.2 Deferred (later)
- **Rust game logic and modules, dev only.** Dynamic library with state snapshots. Constraints from research [V]: no generic reloaded functions, global state re-initialised, no type IDs across the boundary, layout changes need migration. Returns after the module ABI exists.
- **Map scenes.** A light reload-on-save, matching objects by stable path. Deferred by owner decision.

### 13.3 What a Rust change costs in alpha 1
A change to Rust game logic means `engine run` rebuilds and restarts. Rebuild time is measured in Phase 1.5 and reported.

---

## 14. Performance budgets (alpha 1)

### 14.1 Reference machine [TARGET]
One mid-range desktop GPU from 2020 or later, a six-core-or-better CPU, 16 GB RAM, running Windows. Exact models recorded in `benches/REFERENCE.md` before benchmarks count.

### 14.2 Frame budget at 60 FPS (16.67 ms total; lanes overlap)

| Lane | Budget |
|---|---|
| Main thread (scripts, scene, input, command buffer) | 6 ms (scripts at most 2 ms) |
| Physics, amortised | 3 ms |
| Animation | 2 ms |
| Audio update (main side) | 0.5 ms |
| Render submission (CPU) | 2 ms |
| Signals and misc | 0.5 ms |
| GPU frame | 12 ms |

### 14.3 Other targets [TARGET]
- Per-object overhead excluding components: at most 64 bytes.
- Cold start of the base project to first frame: at most 2 s.
- 99th percentile frame time within 1.5 times the mean over a 10-minute soak.
- `engine run` rebuild after a Rust change: measured in Phase 1.5.

---

## 15. Editor (alpha 1, optional, separate)

### 15.1 Principle
The editor is a separate program built on gpui (D-05). It is optional. Everything it does must also be possible from the CLI. The engine never depends on the editor.

### 15.2 Relationship to the engine
- The scene file is the contract. The editor reads and writes the same text format the CLI uses.
- The editor can run a game as a child process and attach to it over a local, versioned channel. Attaching is for live inspection. Authoring always goes through the scene file.
- Protocol changes bump a version. The editor refuses to attach to an engine with a different protocol version and says so.

### 15.3 Features
- Scene tree with child objects and drag-and-drop parenting.
- Inspector for component fields and script parameters, using the derive macro's schema.
- Scripts base object browser: lists base scripts and project modularScripts, and attaches them to objects.
- Map making: grid and snapping, prefab placement with overrides, copy, paste, duplicate, mirror, undo and redo through a command stack, multi-select, group transform.
- Light emitter placement with a preview of light radius.
- Audio source placement with a preview of attenuation range.
- Live inspection of a running game: object values, profiler, console.
- Opens and saves scenes; never writes anything the CLI cannot read.

### 15.4 CLI parity
Every editing action has a CLI equivalent that edits the scene file, for example `engine scene add`, `engine scene set`, `engine scene attach-script`. An editor action that cannot be done from the CLI is a bug, and a CI test checks the pairs.

### 15.5 Constraints
- **gpui version.** The crates.io release `gpui` 0.2.2 (October 2025) is the latest official release seen in this research pass [V, crates.io, secondary confirmation]. Community snapshot crates (`gpui-pre`, 0.3.x, built from Zed revisions) exist and are not official. The editor pins a Zed git revision (D-05). [approved]
- **Windows.** The Windows backend is the `gpui_windows` crate in Zed's repository. Windows needs no feature flags. Text uses DirectWrite [V, gpui-pre README]. The earlier claim of DirectX 11 rendering is not confirmed. [X]
- **Entry point.** Application code starts with `gpui_platform::application().run(...)`, not a bare `gpui` import [V]. The editor's first phase (9.1) uses this form.
- gpui has no mobile target in the reviewed docs, so the editor is desktop-only [V as absent].
- Editor and game run in separate windows (D-05).

---

## 16. CLI and project workflow (alpha 1)

### 16.1 Commands
- `engine new <n>`: creates a project with the base project (World, Player, Camera, Scripts base object).
- `engine build [--release]`: compiles the project.
- `engine run`: builds and runs. Shader and asset changes reload live. Rust changes need a rebuild (Section 13.3).
- `engine check`: validates scenes and scripts, including undeclared service use, duplicate sibling names, and missing references.
- `engine scene ...`: CLI editing of scene files (Section 15.4).
- `engine package [--release]`: writes the package and its report.
- `engine report`: lists the Scripts base object contents, component types, and enabled features.
- `engine bench`: runs the benchmark suite for the project's enabled features.
- `engine editor`: launches the editor, if installed. The CLI works without it.

### 16.2 Project layout
```
my-game/
  engine.toml          enabled features, target platforms, reference settings,
                       backend choice (Section 7.1), audio device (Section 9)
  scenes/              text scene files
  scripts/             Rust source for scripts and modularScripts
  components/          Rust source for project components
  assets/              source assets and their import settings
  shaders/             WGSL
```

---

## 17. Testing
- Each subsystem has a test crate. Physics: conservation checks, known collision outcomes, joint limits. Storage: round trip, corruption detection, and (alpha 2) tampering detection. Audio: voice limits and attenuation curves, run on the mock backend in CI.
- Property tests for scene round-trips and storage schemas.
- Determinism: record, replay, compare state hashes, in the deterministic profile (Section 5.4).
- Packaging: each leaf in a minimal project; the package report is checked for unexpected code (Section 12.3).
- Hot reload: a shader edit and an asset edit each tested end to end, including a failed edit that must keep the previous version.
- CLI parity: CI checks every editor action has a CLI pair (Section 15.4).
- Every target in Section 14 has a benchmark. A regression above 10 percent blocks merging.
- CI on Windows and Linux: build, `clippy`, `rustfmt`, unsafe count, dependency audit, example projects run headless, engine builds with no editor present.

---

## 18. Platform strategy
- **Alpha 1 desktop:** Windows 10 and later, Linux x86-64 with Wayland and X11. `wgpu` uses DX12 on Windows and Vulkan on Linux, selected as in Section 7.1 [DESIGN].
- **Mobile (later):** Vulkan on Android, Metal on iOS [V]. The CPU path is required, since GPU-driven features are partial on Metal and unreliable on some Android devices (Bevy 0.19 notes [V]).
- **Web (later):** possible through `wgpu`'s WebGPU and WebGL2 backends [V].

---

## 19. Roadmap

Each phase ends with something runnable. Effort is relative. Phases marked **[changed]** differ from Revision 5.

### Engine alpha 1

**Phase 0: Research and spikes (medium)**
- **0.1** Repository, toolchain, CI, style rules. *Exit: CI passes on Windows and Linux.*
- **0.2** `wgpu` triangle on Windows (DX12) and Linux (Vulkan). *Exit: triangle renders.*
- **0.3** `hecs` versus `bevy_ecs` on our access patterns (D-11). Read the hecs entity layout to decide the identity choice (Section 4.5). *Exit: decision and identity choice confirmed or revised.* **[changed]**
- **0.4** Rapier spike: pinned-version determinism with a golden hash in the deterministic profile (Section 5.4), a test of `parallel` under determinism, multibody joints for a tree ragdoll, 1,000-body and 160-ragdoll benchmarks (D-04). *Exit: decision record updated with the pinned version.* **[changed]**
- **0.5** Ragdoll feasibility: test Rapier multibody springs and damping against the PD controller (Section 8.2), and estimate the impulse-joint path for any loop. *Exit: go, or change of plan, written down.* **[changed]**
- **0.6** Audio spike: `kira` on `cpal` for Windows and Linux, 128-voice test, mock backend in CI. *Exit: decision confirmed.*
- **0.7** Scheduler spike: custom scheduler versus `bevy_ecs` scheduler on one workload (Section 4.10). *Exit: decision recorded before Phase 2.8.* **[new]**

**Phase 1: Core (medium)**
- **1.1** Platform layer. *Exit: window with input logging on both platforms.*
- **1.2** Math library and SIMD benchmark. *Exit: chosen math type, matching the physics backend.* **[changed]**
- **1.3** Job system, benchmarked against `rayon`, and a check of thread-pool sharing with Rapier. *Exit: scaling numbers.* **[changed]**
- **1.4** Profiler with zones. *Exit: sample frame profile.*
- **1.5** Dev and release profiles, and a rebuild-time benchmark for `engine run`. *Exit: rebuild times recorded.*

**Phase 2: Object, component, and script model (large)**
- **2.1** Objects with IDs, stable paths, sibling uniqueness, and child objects. *Exit: 100,000 objects, stale-handle tests, deep nesting test.*
- **2.2** Plugin API (D-17), `engine-macros` derive, and component registry. *Exit: a sample subsystem registers a component through it.* **[changed]**
- **2.3** Lifecycle, with `mainScript` defaulting to empty, `on_enable` and `on_disable`, and the command buffer. *Exit: callbacks logged in order; child-first unload test.* **[changed]**
- **2.4** `scripts`, `modularScripts`, the Scripts base object, and the `Scripts` component, with per-attachment state. *Exit: one modularScript on ten objects keeps ten states.*
- **2.5** Reusable objects with overrides (Section 4.7). *Exit: prefab instanced 1,000 times with one override.*
- **2.6** Signals with typed payloads and next-stage delivery (Section 4.6). *Exit: test suite and dispatch benchmark.*
- **2.7** Services through the compile-time typed context, and script failure rule (Section 4.8). *Exit: undeclared service use does not compile; a panicking script is disabled in dev and stops release.* **[changed]**
- **2.8** Scheduler with dependencies, per the Phase 0.7 decision. *Exit: parallel systems pass tests.*
- **2.9** Base project (Section 4.11) and `engine new`. *Exit: new project runs an empty level.*

**Phase 3: Scene format and CLI (medium)**
- **3.1** Text scene format with component names, schema hashes, and content hashes (I-5). *Exit: 2,000-object scene round-trips with identical hashes.* **[changed]**
- **3.2** `engine build`, `run`, `check`, `report`. *Exit: a project with two scenes builds and runs.*
- **3.3** `engine scene` editing commands. *Exit: a scene built entirely from the CLI matches a hand-written one.*

**Phase 4: Rendering, shaders, and hot reload (large)**
- **4.1** Renderer skeleton with render graph and backend selection (Section 7.1). Pins the `wgpu` version. *Exit: lit cube on Windows and Linux.* **[changed]**
- **4.2** WGSL pipeline, content-hash cache, and shader hot reload. *Exit: shader edit visible within 300 ms; a broken edit keeps the previous pipeline.*
- **4.3** Basic PBR materials. *Exit: material gallery.*
- **4.4** CPU culling and LOD. *Exit: culling benchmark on a dense scene.*
- **4.5** Asset hot reload with dependency ordering and failure handling. *Exit: texture and material edits without restart; a failed edit keeps the previous version.*

**Phase 5: Lighting core (medium)**
- **5.1** Lighting core leaf with the disabled path (constant ambient from `Environment`). *Exit: a project builds with lighting off and no lighting code in the package report.*
- **5.2** Directional, point, and spot lights with physical units and exposure (Section 7.3). *Exit: 500 lights within budget.* **[changed]**
- **5.3** Light emitter component, including as children. *Exit: a lantern follows a character's hand.*
- **5.4** Light transmittance query interface. *Exit: interface tested with a stub.*
- **5.5** Image-based lighting: prefiltered diffuse and specular maps from an environment map, split-sum method. *Exit: a metal sphere reflects the environment map; prefilter time recorded at load.* **[new]**

**Phase 6: Physics core (large)**
- **6.1** Shapes and broadphase. *Exit: 1,000 bodies on a static mesh.*
- **6.2** Solver with warm starting. *Exit: stacking stable for 60 s.*
- **6.3** Joints and limits. *Exit: 20-link chain.*
- **6.4** Child-object physics rules (Section 4.2). *Exit: a body with a collider child and a body child behaves as specified.*
- **6.5** Continuous collision and character controller. *Exit: fast bullet test, walking on slopes.*
- **6.6** Deterministic stepping and replay in the deterministic profile. *Exit: replay matches state hashes.*
- **6.7** Trace recorder and parameter test matrix. *Exit: bad frame dumped; matrix passes.*

**Phase 7: Animation and ragdoll (large)**
- **7.1** Skeleton import and clips. *Exit: looping walk.*
- **7.2** Blending and a simple state machine. *Exit: smooth locomotion.*
- **7.3** Ragdoll: body-part children, mapping, handoff with velocity matching. *Exit: fall with no visible pop.*
- **7.4** Driven pose (spring or PD, per Phase 0.5) and hit reactions. *Exit: shot character reacts.*
- **7.5** Recovery blend to a rest pose. *Exit: fallen character stands up.*

**Phase 8: Audio, input, camera, storage core (large)**
- **8.1** Audio core on `kira` and `cpal`, 3D sound, device selection. *Exit: 128 positioned voices within budget on Windows and Linux.*
- **8.2** Input mapping with rebinding. *Exit: rebind and replay.*
- **8.3** Camera rigs and the `Camera` component. *Exit: follow and first-person samples.*
- **8.4** Storage core: platforms, data types, atomic writes. *Exit: crash test leaves no half-written file.*
- **8.5** Compile-time tiers (`Normal`, `Secure` marker) and storage service. *Exit: compile-fail test blocks secure to normal assignment; a script saves and loads through the service.*

**Phase 9: Editor (large)**
- **9.1** Editor shell on gpui, pinned Zed revision, entry point per Section 15.5. *Exit: opens a scene file and shows the object tree.*
- **9.2** Scene tree with child objects and drag-and-drop parenting. *Exit: moving a child in the editor changes the scene file correctly.*
- **9.3** Inspector (from the derive schema) and Scripts base object browser. *Exit: attach a base script to an object; the file matches the CLI result.*
- **9.4** Map making: grid, snapping, prefabs, copy, paste, undo and redo. *Exit: build a level; the undo stack reverts every command.*
- **9.5** Light emitter and audio source placement with previews. *Exit: placed objects appear in the running game at the set positions.*
- **9.6** Live inspection of a running game over the versioned channel. *Exit: an object value changes in the inspector while the game runs.*
- **9.7** CLI parity check (Section 15.4). *Exit: every editor action has a passing CLI pair.*

**Phase 10: Packaging and alpha 1 release (medium)**
- **10.1** `engine package` and the package report. *Exit: report lists enabled features, components, and sizes.*
- **10.2** Leaf-independence CI checks. *Exit: each leaf in a minimal project passes.*
- **10.3** Documentation for every alpha 1 public API with an example. *Exit: docs reviewed.*
- **10.4** Alpha 1 tag. *Exit: a sample game built and packaged from the CLI runs on Windows and Linux; the same project opens in the editor.*

### Engine alpha 2

- **A2.1** Shadows (directional cascades, then point and spot leaves). *Exit: stable shadow sample.*
- **A2.2** Baking: `engine bake` writes lightmaps. *Exit: baked scene matches reference within tolerance.*
- **A2.3** IK and foot placement. *Exit: characters walk on stairs.*
- **A2.4** Storage encryption leaf (`storage-crypto`). *Exit: tampering test fails on any changed bit; open-time table per Argon2id profile.*
- **A2.5** Audio occlusion. *Exit: sound behind a wall is attenuated in a test scene.*
- **A2.6** In-game UI. *Exit: HUD sample within budget.*
- **A2.7** Navigation: `rerecast` navmesh and A* pathfinding, compared with a C++ reference scene. *Exit: polygon counts within tolerance.*
- **A2.8** Module ABI (D-16) and a template module. *Exit: a module built from the template loads in release.*

### Checkpoint
After Phase 5, if alpha 1 is more than 30 percent behind its schedule, the editor (Phase 9) moves to alpha 2. [approved]

### Later (unscheduled)
Skybox and procedural sky; map generation and streaming; navmesh crowds; particles; volumetric smoke; the shooter module (phased separately); hot reload of Rust game logic and modules; hot reload of map scenes; WebAssembly sandboxing; GPU-driven culling and meshlets; motion matching; networking; Android; iOS.

---

## 20. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Alpha 1 is large (editor, audio, IBL, hot reload, components) | High | High | Checkpoint after Phase 5 (Section 19); public release tag only after Phase 10 |
| Ragdoll needs joints Rapier cannot provide | Low | High | Multibody trees confirmed for tree ragdolls; loops fall back to impulse joints; Phase 0.5 gate |
| Rapier determinism differs from docs, or `parallel` breaks it | Medium | High | Deterministic profile without SIMD; Phase 0.4 golden-hash test on pinned version |
| Component and reflection derive is complex | Medium | Medium | Derive limited to declared fields; tests in Phase 2.2 |
| Scheduler cost | Medium | Medium | Phase 0.7 comparison with `bevy_ecs` before Phase 2.8 |
| Editor and CLI diverge | Medium | High | CLI parity rule and CI pair check (Section 15.4) |
| Scene format changes break the editor | Medium | Medium | Versioned format, migration tests |
| gpui churn; official crate is stale | High | Medium | Pinned Zed revision; editor is optional |
| Asset and shader hot reload leaves the engine in a bad state | Medium | Medium | Failure rule keeps the previous version; end-to-end tests (Section 17) |
| Audio tested mainly on Windows | Medium | Medium | Phase 0.6 and 8.1 include Linux; mock backend in CI |
| Packaging does not remove code as expected | Medium | Medium | Package report and leaf-independence CI (Phase 10.2) |
| Without Rust hot reload, iteration is slow | Medium | Medium | Rebuild time measured in Phase 1.5; logic hot reload is a later item |
| Storage without encryption is used for secrets | Medium | High | Docs; `Secure<T>` rejected without the encryption leaf (Section 11.1) |
| Rhai benchmark figures are out of date | Low | Low | Figures updated in Appendix B; Rhai is not used in alpha 1 |

---

## 21. Open items

All decisions in Appendix A are confirmed by the owner, except items that depend on measurement ("verify") and the following:

1. **Reference machine.** Choose the hardware for Section 14.1, so benchmarks are comparable.
2. **Items still [X].** Listed in Appendix B.7. Each has a phase that checks it.
3. **Procedural sky and skybox.** Deferred by owner decision; to be scheduled later.

---

# Part II

## Appendix A: Decision records

Criteria used for every decision: customisability, performance, and best (the strongest option the evidence supports). Native code only with a clear reason.

| ID | Decision | Choice | Status |
|---|---|---|---|
| D-01 | Foundation | Own engine built from Rust crates | Confirmed |
| D-02 | Graphics | `wgpu`, latest release pinned at Phase 4.1 (29.0.1 seen in research) | Confirmed |
| D-03 | Script authoring | Rust only; shaders are WGSL | Confirmed |
| D-04 | Physics | Rapier behind `PhysicsBackend`; Jolt not used; version pinned at Phase 0.4 (0.34.0 seen in research) | Confirmed; verify in Phase 0.4 |
| D-05 | Editor UI | gpui, pinned to a Zed git revision; optional, separate; alpha 1 | Confirmed |
| D-06 | In-game UI | Own UI on `wgpu` (alpha 2) | Confirmed |
| D-07 | Storage encryption | AES-256-GCM with AES hardware, ChaCha20-Poly1305 otherwise (alpha 2) | Confirmed |
| D-08 | Physics timestep | 1/120 s default, 1/60 s profile | Confirmed |
| D-09 | Licence | Dual MIT OR Apache-2.0 | Confirmed |
| D-10 | Archive compression | zstd, benchmarked against lz4 | Confirmed; verify |
| D-11 | ECS | `hecs` (0.11.1 seen in research) under our scheduler and registry | Confirmed; verify in Phase 0.3 |
| D-12 | Navmesh | `rerecast` (0.3.2 seen in research), alpha 2 | Confirmed; verify |
| D-13 | Audio | `cpal` with `kira` (0.12.0 seen in research) behind `AudioBackend` (alpha 1) | Confirmed; verify in Phase 0.6 |
| D-14 | Shaders | WGSL, hot reloaded in alpha 1 | Confirmed |
| D-15 | Comments | No comments in source, except `// SAFETY:`; docs included by attribute | Confirmed (revised, B3) |
| D-16 | Module ABI | Versioned `repr(C)` entry with profile check (alpha 2) | Confirmed |
| D-17 | Plugin API | One API for subsystems and modules | Confirmed |
| D-18 | Script model | mainScript, scripts, modularScripts; compile-time typed service context | Confirmed |
| D-19 | Hot reload | Shader and asset reload in alpha 1; Rust logic and map scenes deferred | Confirmed |
| D-20 | Packaging | Leaf features, static release, package report | Confirmed |
| D-21 | Random | `rand` 0.9.4 for generators; named reproducible PRNG for simulation; own noise module; base script | Confirmed (revised) |
| D-22 | Storage split | Core in alpha 1 with `Secure<T>` marker; encryption leaf in alpha 2 | Confirmed (revised, B1) |
| D-23 | Signals versus references | Rule in Section 4.6 | Confirmed |
| D-24 | State on overrides | Rule in Section 4.7 | Confirmed |
| D-25 | Lighting | Own submodule; core leaf; physical units; spot lights; IBL in alpha 1; shadows and baking alpha 2 | Confirmed (revised) |
| D-26 | Child objects | Objects contain child objects; physics rule in Section 4.2 | Confirmed |
| D-27 | Editor | Optional separate program; CLI parity required | Confirmed |
| D-28 | Components | Plain data components with derive macro; `Scripts` component; `LightEmitter` with kind field | Confirmed (new) |
| D-29 | Skybox | Deferred; no skybox in alpha 1 | Confirmed (new) |
| D-30 | Determinism profile | `enhanced-determinism` without SIMD; `parallel` tested in Phase 0.4 | Confirmed (new) |

### A.1 Rationale for the main decisions

**D-01 Foundation.** Bevy and Fyrox impose their own object models. Our object and component model is specific, and customisability is the first criterion. Proven crates underneath keep performance and maturity. Fallback: Fyrox, if Phase 4 slips badly.

**D-02 Graphics.** `wgpu` covers Vulkan, Metal, D3D12, and OpenGL from one codebase, which the mobile goal needs later. Raw Vulkan would need separate D3D12 and Metal paths. Windows uses DX12 by choice, since `wgpu` enables all backends by default [V].

**D-03 Scripts in Rust.** Native speed and full customisation. Lua is C and rejected by the Rust-only rule. Rhai is pure Rust and sandboxed, but its own documentation describes it as an embedded scripting language, so it is kept for data-driven tuning, not core game logic. WebAssembly stays for sandboxed user code later.

**D-04 Physics.** Rapier is pure Rust, its source can be forked, and its maintainer reports it faster than Jolt (self-reported). Jolt is C++, and the evidence does not show a benefit that outweighs the rule. Jolt remains a design reference. Revisit only if the Phase 0.4 benchmark misses the physics budget by more than 50 percent.

**D-05 Editor UI.** gpui is the most capable Rust-native option for a tools UI in the evidence. Its official crates.io release is stale, so the editor pins a Zed git revision. Because the editor is optional and separate, gpui churn cannot break the engine.

**D-06 In-game UI.** A shipped game should not depend on a pre-1.0 editor framework. Editor and game share design tokens, not code.

**D-07 Encryption.** AES-GCM is faster where AES hardware exists. ChaCha20-Poly1305 is the portable choice. The header records which one a file uses.

**D-08 Timestep.** 1/120 s suits hit accuracy in the shooter. 1/60 s halves physics cost for weaker machines.

**D-10 Compression.** Decode speed matters for streaming. Choose lz4 if its decode time per megabyte is less than half of zstd's and its ratio is within 15 percent; otherwise zstd.

**D-11 ECS.** `hecs` is a library, not a framework, and leaves scheduling to us. Its current release is 0.11.1 [V, docs.rs]. `bevy_ecs` is the fallback if Phase 0.3 shows it wins by more than 25 percent on our patterns and the coupling cost is acceptable.

**D-12 Navmesh.** `rerecast` is a pure-Rust Recast port with tiles, polygon and detail meshes, and a `libm` feature for cross-platform determinism [V, rerecast docs]. The Bevy integration is not used, since the engine has no Bevy dependency. The C++ wrapper crate is excluded by the Rust-only rule.

**D-13 Audio.** Kira provides tweens, tracks, effects, and spatial audio on `cpal`. Its mock backend allows CI audio tests without a device [V]. The `AudioBackend` trait allows replacement if voice limits or occlusion need more.

**D-14 Shaders.** `wgpu` consumes WGSL directly, which makes shaders hot-reloadable without a Rust compile step.

**D-15 Comments.** The no-comments rule applies to source, except `// SAFETY:` on `unsafe` blocks, which Clippy requires. Documentation in `docs/`, included by an attribute, keeps rustdoc while leaving source clean.

**D-16 Module ABI.** A `repr(C)` entry with an API version and build profile gives explicit failure modes. The pattern follows an open-source Bevy-based editor [V]. Alpha 2.

**D-17 Plugin API.** One API means the shooter exercises exactly what built-in subsystems use.

**D-18 Script model.** Three kinds with clear ownership cover every case discussed. Services are capabilities passed through a typed context, so undeclared use fails to compile.

**D-19 Hot reload.** Shader and asset reload read data at runtime and need no code loading, so they are cheap and safe enough for alpha 1.

**D-21 Random.** `rand` 0.9.4 is the current release [V, docs.rs]. `rng()` is the thread-local generator and is not reproducible across runs, so simulation uses a named seeded PRNG from `rand_chacha` or `rand_xoshiro` [V]. `rand` has no noise functions, so the noise module is ours.

**D-22 Storage split.** A core that is always present and an encryption leaf keep unencrypted projects free of crypto code. The `Secure<T>` marker in alpha 1 keeps the compile-time guarantee without the crypto.

**D-25 Lighting.** A submodule with a core leaf keeps lighting optional, and "no lighting" means constant ambient, not an error. Physical units match PBR and make tuning predictable once exposure is set. IBL is in alpha 1 so PBR materials look right before shadows exist.

**D-26 Child objects.** One composition rule covers bodies with heads and arms, lanterns in hands, and ragdolls.

**D-27 Editor.** The scene file is the contract. Requiring CLI parity keeps the engine complete without the editor and stops the editor from holding state the engine cannot read.

**D-28 Components.** Data-only components keep behaviour in one place (scripts and systems), make the scene file the single source of state, and let the editor inspect any component through the derive schema. A `Scripts` component connects scripts to the ECS without a second object store.

**D-29 Skybox.** Deferred by owner decision. Alpha 1 IBL carries the lighting value without the backdrop.

**D-30 Determinism profile.** Rapier's source refuses SIMD with `enhanced-determinism` [V], so the deterministic profile gives up SIMD. The cost is measured in Phase 0.4 against the physics budget.

### A.2 Reversal triggers
- **D-04:** the Phase 0.4 benchmark misses the physics budget by more than 50 percent, or determinism fails on the pinned version.
- **D-11:** `bevy_ecs` wins by more than 25 percent on our patterns with acceptable coupling.
- **D-05:** the pinned gpui revision fails to build on either platform for two consecutive phase boundaries.
- **D-13:** Phase 0.6 shows `kira` cannot meet the 128-voice target on Linux or cannot express the occlusion design.
- **D-22:** the compile-time tier check proves awkward in Phase 8.5; a runtime check replaces it.
- **D-20:** dead-code elimination leaves more than 10 percent of disabled-feature code in a minimal package.
- **D-19:** shader or asset reload causes state corruption in Phase 4.2 or 4.5 that cannot be fixed by the failure rule.
- **D-30:** the deterministic profile misses the physics budget by more than 50 percent, in which case the profile is dropped from alpha 1 and recorded here.
- **Ragdoll:** Phase 0.5 finds the tree ragdoll cannot meet its targets with multibody joints and the impulse-joint fallback is too costly for alpha 1.

---

# Part III

## Appendix B: Research evidence

### B.1 Method and tags
Sources were searched and read during research. The research pass for Revision 6 was run on 24 November 2026. [V] means verified from an official or primary source. [S] means secondary or self-reported. [X] means not verified. Benchmarks are relative evidence only; none was reproduced on the reference machine. Where sources conflict, both are recorded.

### B.2 Engines

**Counter-Strike 2 (Source 2).**
- Smokes are volumetric, pushed by grenades and bullets, react to light, collide with the world, and are cleared by nearby explosions [V: Valve wiki; S: coverage].
- CS2 uses sub-tick input timing so tick rate does not limit movement, shooting, or throwing [V].
- Community analysis (not verified): voxel density grid, flood fill from detonation, GPU density updates with noise, bullet holes as density sinks, per-pixel ray marching, quarter-resolution rendering [S/X].
- Take: one density field for rendering, vision, and bullets. Avoid: pre-baked smoke textures [S].

**Unreal Engine (Nanite).** Clusters of triangles are built at import, swapped by view at varying detail, streamed on demand, and drawn through a dedicated pass. Performance depends on content and hardware, so the plan measures per scene [V].

**Bevy.**
- Meshlet rendering from 0.14. GPU transform and culling from 0.16. More GPU work, GPU light clustering, and parallel mesh collection from 0.19 (about 4 ms saved with 200,000 moving meshes in one benchmark) [V].
- Meshlets need 64-bit texture atomics. Metal support is partial and WebGL 2 has none [V].
- Bevy 0.20 release date: [S, 8 October 2026, not re-checked this pass]. [X]
- Take: GPU-driven culling with CPU fallback, and the table versus sparse-set storage split.

**Fyrox.**
- Scene nodes in memory pools. Rust scripts on nodes. Dynamic-library hot reload. Rapier physics [V].
- Version 0.36: hierarchical property propagation scales with changes, not graph size [V].
- Version 1.0.0, March 2026 [S, not re-checked this pass]. [X]
- Take: dirty-flag propagation, and the option of a shared UI library. Avoid: a scene model coupled to its editor.

**Jolt (C++, MIT).** Multithreaded islands, sleeping, ragdoll skeleton mapping, CCD, deterministic simulation, 1/120 s step with 60 Hz generally stable, platforms including Android and iOS [V, earlier pass]. Requires C++ [V]. Not re-checked this pass. [X]

**Recast and Detour (C++, zlib).** Voxel rasterisation, region partitioning, tiled navmeshes, temporary obstacles, crowds. Standard in major engines [V].

**Unity and Godot.** Not verified in this pass [X]. Used only for general lessons; re-check before any idea is built on them.

**Godot Rapier Physics (community).** A Godot plugin built on Rapier with variants for "fast parallel SIMD" and "enhanced deterministic" builds. Its published variant table excludes `parallel` and `simd-stable` from the deterministic builds [S, asset store page; version 0.8.40 indexed recently]. Used as evidence that the feature combinations are a known trade-off, not as a benchmark.

### B.3 Libraries

**Physics.**
- Rapier `rapier3d` latest seen on docs.rs: 0.34.0 [V]. The earlier figure 0.35.3 is not confirmed [X].
- Determinism source facts [V, rapier2d 0.32.0 and 0.33.0 source; rapier3d 0.13.0 source]: `enhanced-determinism` with `simd-is-enabled` produces a compile error. Snapshot and restore are supported [V, docs.rs].
- `parallel` under `enhanced-determinism`: not refused by the source; the asset variants exclude it. Phase 0.4 settles it [X].
- Multibody joints: tree-structured; closed loops are not supported and must use impulse joints [V, rapier3d-mjcf 0.34.0 and bevy_rapier3d 0.26.0 documentation].
- Multibody implicit joint springs and per-DoF damping: [V, rapier3d 0.34.0 test source].
- Maintainer benchmark (undated in our copy): about 50 percent faster than Jolt single-threaded and about 100 percent faster multi-threaded [S, self-reported].
- `rapier3d-urdf` 0.13.0 and `rapier3d-mjcf` 0.34.0 are loaders that use multibody joints and document their limits [V]. Useful as reference for robot-style articulated bodies.
- `oxijolt` wraps Jolt's C layer and inherits C++ [V].

**Rendering.** `wgpu` 29.0.1 on docs.rs (`wgpu-types` 29.0.1) [V]. Backends: Vulkan, Metal, D3D12 (Dx12), OpenGL (Gl), WebGPU [V]. `Backends::default()` enables all backends [V]. The `StoreOp` API replaces the boolean store setting [V, wgpu changelog]. Naga translates WGSL [V]. Vello 0.8.0 (2026-03-20) uses wgpu 28; `imaging_wgpu` 0.0.2 (2026-05-30) defaults to wgpu 29 [V, changelogs]. The version 30.0.1 is not confirmed [X].
- Windows backend choice: DX12 by explicit configuration; not the default [V, wgpu-types docs list all backends by default].

**Editor UI.**
- `gpui` official crates.io release: 0.2.2, 22 October 2025 [V, secondary confirmation]. Still the latest official release in this pass.
- Community snapshots `gpui-pre` (0.3.3, 0.3.6, 0.3.8 cited) are generated from Zed revisions and are not official [S].
- Entry point: `gpui_platform::application().run(...)` [V, gpui-pre README]. Windows needs no features; text uses DirectWrite [V, gpui-pre README].
- Windows backend: `gpui_windows` crate in Zed's repository [V, gpui-pre-windows source].
- DirectX 11 rendering claim: not confirmed [X].

**ECS.**
- `hecs` 0.11.1 is the latest seen on docs.rs [V]. Library, not a framework; `no_std`-capable; Apache-2.0 or MIT [V, earlier pass].
- `bevy_ecs`: full ECS tied to Bevy's App model [V, earlier pass].
- Benchmark sources: a community suite; results differ by workload [S].
- Scheduler comparison: Phase 0.7 [X].

**Navigation.** `rerecast` 0.3.2 is the latest seen [V]: pure-Rust Recast port; polygon and detail meshes; tiles; watershed, monotone, and layer partitioning; `no_std` support; cross-platform determinism through the `libm` feature [V, rerecast docs]. Compatible with Bevy 0.17 and 0.18 through `bevy_rerecast` 0.3 and 0.4 [V]; the engine does not use the Bevy integration. Off-mesh links and pathfinding companions: [X]. `recast_navigation` wraps C++ and is excluded [V].

**Audio.**
- `kira` 0.12.0 is the latest seen on docs.rs [V]. Backend-agnostic. `cpal` feature on by default. Backend trait with `setup` and `start`; `CpalBackendSettings` allows a chosen device and stream config [V]. Mock backend available [V]. `assert_no_alloc` feature available for development [V]. Symphonia provides additional formats [V].
- The `kira-cpal` crate is an unstable 2021 beta, superseded by the built-in cpal backend. Not used [V, lib.rs].
- Tested mainly on Windows [V, earlier pass].

**Scripting.**
- Rhai 1.27.0 is the latest release seen [V, GitHub release page, October]. Pure Rust, no-std capable, sandboxed. Supports progress tracking and manual termination of scripts, a "don't panic" guarantee, disabling of language features, and a bytecode compiler (Rhai Grain), which is no longer experimental in 1.27.0 [V].
- Benchmark: one million iterations in about 0.3 s on a single-core 2.3 GHz Linux VM [V, Rhai README]. The earlier figure of 0.14 s on a 2.6 GHz VM is superseded [V].
- The docs describe Rhai as suitable for scripting and DSL use. The earlier note "docs advise against full game logic" is not in the current text [X].
- Wasmtime: pure Rust on Cranelift. Host-to-wasm overhead about 20 to 30 ns and wasm-to-host about 3 to 5 ns in an older issue [S, dated; not re-checked this pass]. [X]
- Lua via `mlua`: C [V]. Excluded.

**Cryptography.**
- RFC 9106: Argon2id required. Profiles: high security (m = 2 GiB, t = 1, p = 4) and memory-constrained (m = 64 MiB, t = 3, p = 4) [V, earlier pass].
- `crypt-io` benchmarks (self-published): AES-256-GCM about 1.55 GiB/s at 64 KiB; ChaCha20-Poly1305 about 1.45 GiB/s at 64 KiB on one reference machine [S].
- ChaCha20-Poly1305 (RFC 8439) and AES-256-GCM primary specifications: [X, Phase 11.3].

**Random.**
- `rand` 0.9.4 is the latest seen [V, docs.rs].
- Renamed and new top-level functions in 0.9.0: `rng()`, `random_range`, `random_bool`, `random_ratio`, `random_iter`, `fill`; `Rng::random`, `random_range`, `random_bool`, `random_ratio` [V, rand 0.9.0 changelog].
- `rand` documents portable, reproducible output for many algorithms, but not all, and recommends the book's portability guidance [V, rand README and docs]. Generators named in the docs include `rand_chacha`, `rand_xoshiro`, and `rand_pcg` [V].
- No noise functions [V].

**Hot reload.** `hot-lib-reloader`: non-generic reloaded functions, global state re-initialisation, `TypeId` changes across reload, layout changes break reload, state snapshots as the workaround [V, earlier pass]. Module ABI pattern from an open-source Bevy editor (`jackdaw`): API version, Bevy version, and profile checks [V, earlier pass].

**Packaging.** Dead-code elimination behaviour is not researched as a separate source. The rule rests on linking behaviour and is checked in Phase 10.2 [X].

**Spatial hashing and splitmix64.** Coordinate primes and mixing constants are widely published [S]; confirm against a primary source before use in mapgen [X].

### B.4 Sources
1. Valve Developer Community, Counter-Strike 2 and Cs2 pages. https://developer.valvesoftware.com/wiki/Cs2
2. Spieltimes, CS2 smoke coverage. https://www.spieltimes.io/news/counter-strike-2-smokes-system-will-work/
3. Outscal, community analysis of CS2 volumetric smoke (hypothesis only). https://outscal.com/web-story/cs2-volumetric-smoke-physics
4. Jolt Physics repository and documentation. https://github.com/jrouwe/JoltPhysics , https://jrouwe.github.io/JoltPhysics
5. Jolt samples documentation. https://jrouwe.github.io/JoltPhysics/md__docs_2_samples.html
6. oxijolt crate. https://docs.rs/oxijolt
7. bevy_ecs crate. https://docs.rs/bevy_ecs
8. hecs crate, latest. https://docs.rs/crate/hecs/latest
9. Recast Navigation repository. https://github.com/recastnavigation/recastnavigation
10. rerecast crate, latest (0.3.2). https://docs.rs/crate/rerecast/latest
11. recast_navigation crate. https://docs.rs/crate/recast_navigation/latest
12. wgpu crate (wgpu-types 29.0.1). https://docs.rs/crate/wgpu-types/29.0.1/source/src/backend.rs
13. gpui crate. https://docs.rs/crate/gpui/latest ; gpui-pre README. https://docs.rs/crate/gpui-pre/0.3.3
14. gpui issue on stale crates.io release. https://github.com/zed-industries/zed/issues/66275
15. Fyrox engine. https://fyrox.rs ; release notes 0.33 and 0.36
16. Bevy release notes. https://bevy.org/news/bevy-0-14/ , https://bevy.org/news/bevy-0-16/ , https://bevy.org/news/bevy-0-19/
17. hot-lib-reloader crate. https://docs.rs/crate/hot-lib-reloader/latest
18. jackdaw_api_internal ABI source. https://docs.rs/crate/jackdaw_api_internal/0.4.0/source/src/ffi.rs
19. Kampffrosch, hot reloading guide (mirror). https://news.lavx.hu/article/hotreloading-rust-and-so-can-you
20. RFC 9106 (Argon2). https://www.rfc-editor.org/rfc/rfc9106.html
21. JEP 8377081 (quotes RFC 9106 profiles). https://openjdk.org/jeps/8377081
22. crypt-io crate benchmarks. https://docs.rs/crate/crypt-io/latest/source/docs/PERFORMANCE.md
23. Rhai documentation and repository. https://rhai.rs/book/print.html , https://github.com/rhaiscript/rhai , release 1.27.0: https://github.com/rhaiscript/rhai/releases/tag/v1.27.0
24. Wasmtime. https://docs.rs/wasmtime
25. Kira audio, latest (0.12.0). https://docs.rs/kira/latest/index.html ; https://docs.rs/crate/kira/latest/source/src/backend/cpal.rs
26. rand 0.9 documentation and changelog. https://docs.rs/rand/0.9.4/src/rand/lib.rs.html ; https://github.com/rust-random/rand/releases
27. Rapier determinism and feature sources. https://docs.rs/crate/rapier2d/0.33.0/source/src/lib.rs ; https://docs.rs/crate/rapier3d/latest/source/tests/multibody_spring.rs ; https://github.com/dimforge/rapier/blob/master/CHANGELOG.md (confirm exact URLs before citing in public docs)
28. rapier3d-mjcf loader. https://docs.rs/crate/rapier3d-mjcf/latest/source/src/loader/insert.rs
29. rapier3d-urdf loader. https://docs.rs/crate/rapier3d-urdf/latest/source/src/lib.rs
30. bevy_rapier3d joint documentation. https://docs.rs/crate/bevy_rapier3d/0.26.0/source/src/dynamics/joint.rs
31. Godot Rapier Physics plugin (variant table). https://github.com/appsinacup/godot-rapier-physics
32. Mozilla bug 1836816 (DX12 default for WebGPU on Windows, context only). https://bugzilla.mozilla.org/show_bug.cgi?id=1836816
33. Vello changelog. https://skia.googlesource.com/external/github.com/linebender/vello/+/refs/heads/dirty_external/CHANGELOG.md
34. imaging_wgpu changelog. https://docs.rs/crate/imaging_wgpu/latest/source/CHANGELOG.md
35. wgpu changelog (StoreOp, instance flags). https://github.com/SecondHalfGames/wgpu/blob/trunk/CHANGELOG.md
36. gpui-pre-platform and gpui-pre-windows sources. https://docs.rs/crate/gpui-pre-platform/0.3.6/source/Cargo.toml ; https://docs.rs/crate/gpui-pre-windows/0.3.3/source/src/window.rs

Not re-fetched and to be confirmed in their phases: RFC 8439 (ChaCha20-Poly1305), AES-256-GCM specification, zstd and lz4 benchmarks, splitmix64 reference, spatial hash primes, `ecs_bench_suite` results, Bevy 0.20 release details, Fyrox 1.0 details, Jolt latest, Wasmtime overhead figures.

### B.5 Changes in research since Revision 5
- rapier3d latest seen 0.34.0; 0.35.3 not confirmed.
- wgpu latest seen 29.0.1; 30.0.1 not confirmed.
- hecs latest seen 0.11.1.
- kira latest seen 0.12.0, with mock backend and `assert_no_alloc`.
- rerecast latest seen 0.3.2, with a `libm` determinism feature.
- rand latest seen 0.9.4; `rng()` is not for simulation.
- Rhai latest seen 1.27.0; benchmark figure updated.
- Rapier determinism: SIMD and `enhanced-determinism` are incompatible [V].
- Multibody joints: tree-only [V].
- gpui: official 0.2.2 still latest; entry point and Windows facts updated.

### B.6 Research method note
Searches used the live web for version and API facts. Claims about published benchmarks and dates are marked [S] unless the primary source was read.

### B.7 Items still [X], with the phase that checks them
- Rapier `parallel` under determinism: Phase 0.4.
- Rapier cone-twist joints and character controller API: Phase 0.4 and 6.5.
- Rapier multibody driven pose versus PD: Phase 0.5.
- wgpu 30 availability and DX12 path behaviour: Phase 4.1.
- gpui Windows rendering backend details: Phase 9.1.
- Kira `assert_no_alloc` default in debug builds: Phase 8.1.
- Argon2id and AEAD primary specifications: Phase 11.3 (alpha 2).
- Bevy 0.20 details, Fyrox 1.0 details, Jolt latest, Wasmtime overhead, zstd and lz4 benchmarks: when a decision depends on them.
- Math crate choice: Phase 1.2.
- Spatial hash constants: before mapgen (later).
- Off-mesh links and pathfinding companions for `rerecast`: Phase A2.7.

---

# Part III, continued

## Appendix C: Changes from Revision 5

**Approved changes (A and B series).**
- A1: Rapier version no longer pinned in the plan; pinned at Phase 0.4.
- A2: wgpu version pinned at Phase 4.1; store operation API updated.
- A3: gpui entry point and Windows facts updated; Zed revision pin kept.
- A4: hecs version recorded (0.11.1).
- A5: multibody joint limits documented; ragdoll requirement partly verified; driven-pose options added.
- B1: `Secure<T>` marker in alpha 1; compile-time failure without the encryption leaf.
- B2: identity choice moved to Phase 0.3.
- B3: `// SAFETY:` comments allowed.
- B4: alpha 1 checkpoint kept; public release tag after Phase 10.
- B5: scheduler comparison spike added (Phase 0.7).
- B6: spring-based ragdoll drive tried before PD.

**Component and object review (approved recommendations).**
- Section 4 rewritten: objects as hecs entities, components as plain data, `Scripts` component, derive macro for reflection, compile-time service context, script failure rule, stable path order, command buffer, expanded lifecycle.
- `LightEmitter` with a kind field replaces the separate light concept.
- Component catalogue added (Section 4.12).

**Lighting and sky (owner decisions).**
- Physical light units with exposure.
- Spot lights in alpha 1, unshadowed.
- Image-based lighting in alpha 1.
- Skybox deferred.

**Determinism and randomness.**
- Deterministic profile without SIMD (Section 5.4, D-30).
- Named seeded PRNG for simulation (D-21).

**Platform.**
- Windows DX12 by configuration; Vulkan on Linux (Section 7.1).

**Roadmap.** Phases 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, and 10 updated as marked **[changed]** or **[new]** in Section 19.

**Unchanged.** Section 1.1 (vision), Section 6 apart from the math and jobs notes, Section 9 apart from the audio testing notes, Section 13 (hot reload), Section 14 (budgets, except the Windows reference), Section 17 apart from the audio and determinism lines, Section 18 apart from backend choice, Appendix A decisions D-01 to D-27 apart from the revisions listed in the table.
