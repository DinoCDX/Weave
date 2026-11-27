# weave-eye

Owner area: rendering. Boundary: the GPU backend, the render graph, meshes, cameras, lights, and shaders. It depends on `weave-heart` to read objects, and on no editor or compiler code.

Phase E3.1 provides:
- `Backend`: DX12 on Windows and Vulkan on Linux, chosen from the platform key. The choice is explicit, because wgpu enables every backend by default.
- `RenderGraph`: named passes (`clear`, `opaque`, `transparent`, `post`) that declare what they read and write. The graph rejects a read of a resource no earlier pass wrote. Each pass's CPU time is measured, and `report()` prints the table.
- `Renderable`: a component holding a colour. `collect_instances` turns the loaded objects with a `Renderable` and a `Transform` into draw instances.
- `Camera` and `DirectionalLight`, with exposure and ambient terms. The cube shader is WGSL, in `shaders/cube.wgsl`.
- Shader hot reload (E3.2): `use_shader_file` and `reload_shader`. A broken edit keeps the previous pipeline. Pipelines are cached by content hash.
- Batched drawing (E3.3), with frustum culling on the GPU and `draw_indexed_indirect` (D-R4). A depth prepass lays depth down before the opaque pass (D-R5). Two-phase Hi-Z occlusion culling uses a depth pyramid built from phase one (D-R9). Meshes can carry four levels of detail, chosen per instance on the GPU (D-R10; see `add_mesh_lods`). `Renderer::visible_counts()` reads the GPU's draw counts back, for benchmarks and tests.
- Meshes and batches: `Renderer::add_mesh` uploads a `Mesh`; `draw` takes batches of instances, each with its mesh. Built-in meshes: `cube`, `ground`, and `lobed_sphere` (a complex, procedurally displaced sphere). Lights: up to 4,096 point and spot lights, clustered into screen tiles and depth slices (D-109), plus 4 directional lights. `Light::key`, `Light::flood`, and `Light::accent` are helpers for the three roles. Below four storage lights a direct loop is used instead, since it is cheaper there (D-114). Directional shadows (D-118): a shadow map for the first directional light, with a switch (`set_shadows`). Shadow casters are culled against the light's frustum, not the camera's, so an object outside the view still casts into it. Opaque instances in each batch are drawn front to back. Benchmark results are in B-012 to B-016.
- `Renderer`: owns the surface, depth target, pipelines, and buffers. Each frame uploads every submitted instance once, as 96-byte records (model, colour, material), into a buffer that grows only when needed; a compute pass then culls them into compacted buffers for the camera and the shadow map.

The `cube` example in `examples/cube` runs three spinning cubes, driven by a heart script, and `--frames N` exits after N frames with the pass timing report. Benchmark results are in `docs/benchmarks.md`.

Math comes from `glam`, not a project type. Pass times are reported on the CPU and, when the adapter supports `TIMESTAMP_QUERY`, on the GPU (D-96). Lights: directional, point, and spot, up to 16 (D-98). PBR shading (E3.5) is built: metallic-roughness GGX with ACES tone mapping, using a metallic and roughness value per instance (`DrawInstance::with_material`). Image-based lighting (E3.7) and reversed-Z depth are not built (D-R8).
