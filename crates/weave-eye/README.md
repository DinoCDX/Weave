# weave-eye

Owner area: rendering. Boundary: the GPU backend, the render graph, meshes, cameras, lights, and shaders. It depends on `weave-heart` to read objects, and on no editor or compiler code.

Phase E3.1 provides:
- `Backend`: DX12 on Windows and Vulkan on Linux, chosen from the platform key. The choice is explicit, because wgpu enables every backend by default.
- `RenderGraph`: named passes (`clear`, `opaque`, `transparent`, `post`) that declare what they read and write. The graph rejects a read of a resource no earlier pass wrote. Each pass's CPU time is measured, and `report()` prints the table.
- `Renderable`: a component holding a colour. `collect_instances` turns the loaded objects with a `Renderable` and a `Transform` into draw instances.
- `Camera` and `DirectionalLight`, with exposure and ambient terms. The cube shader is WGSL, in `shaders/cube.wgsl`.
- Shader hot reload (E3.2): `use_shader_file` and `reload_shader`. A broken edit keeps the previous pipeline. Pipelines are cached by content hash.
- Batched and unbatched drawing (E3.3), and frustum culling (E3.4) with `FrameStats`.
- `Renderer`: owns the surface, depth target, pipeline, and buffers. Instance data is written once per frame into a buffer that grows only when needed.

The `cube` example in `examples/cube` runs three spinning cubes, driven by a heart script, and `--frames N` exits after N frames with the pass timing report. Benchmark results are in `docs/benchmarks.md`.

Math comes from `glam`, not a project type. GPU timestamp queries are not built yet, so the pass times are CPU wall time. LOD (E3.4), PBR (E3.5), extra lights (E3.6), and image-based lighting (E3.7) are not built.
