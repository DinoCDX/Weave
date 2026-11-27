# weave-eye

Owner area: rendering. Boundary: the GPU backend, the render graph, meshes, cameras, lights, and shaders. It depends on `weave-heart` to read objects, and on no editor or compiler code.

Phase E3.1 provides:
- `Backend`: DX12 on Windows and Vulkan on Linux, chosen from the platform key. The choice is explicit, because wgpu enables every backend by default.
- `RenderGraph`: named passes (`clear`, `opaque`, `transparent`, `post`) that declare what they read and write. The graph rejects a read of a resource no earlier pass wrote. Each pass's CPU time is measured, and `report()` prints the table.
- `Renderable`: a component holding a colour. `collect_instances` turns the loaded objects with a `Renderable` and a `Transform` into draw instances.
- `Camera` and `DirectionalLight`, with exposure and ambient terms. The cube shader is WGSL, in `shaders/cube.wgsl`.
- Shader hot reload (E3.2): `use_shader_file` and `reload_shader`. A broken edit keeps the previous pipeline. Pipelines are cached by content hash.
- Batched and unbatched drawing (E3.3), and frustum culling (E3.4) with `FrameStats`.
- Meshes and batches: `Renderer::add_mesh` uploads a `Mesh`; `draw` takes batches of instances, each with its mesh. Built-in meshes: `cube`, `ground`, and `lobed_sphere` (a complex, procedurally displaced sphere). Lights: up to 4,096 point and spot lights, clustered into screen tiles and depth slices (D-109), plus 4 directional lights. `Light::key`, `Light::flood`, and `Light::accent` are helpers for the three roles. Below four storage lights a direct loop is used instead, since it is cheaper there (D-114). Directional shadows (D-118): a shadow map for the first directional light, with a switch (`set_shadows`). Benchmark results are in B-012 to B-016.
- `Renderer`: owns the surface, depth target, pipeline, and buffers. Instance data is written once per frame into a buffer that grows only when needed.

The `cube` example in `examples/cube` runs three spinning cubes, driven by a heart script, and `--frames N` exits after N frames with the pass timing report. Benchmark results are in `docs/benchmarks.md`.

Math comes from `glam`, not a project type. Pass times are reported on the CPU and, when the adapter supports `TIMESTAMP_QUERY`, on the GPU (D-96). Lights: directional, point, and spot, up to 16 (D-98). LOD (E3.4), PBR materials (E3.5), and image-based lighting (E3.7) are not built.
