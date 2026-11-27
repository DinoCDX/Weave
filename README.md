# WeaveEngineX

A Rust game engine for 3D games on desktop, built around efficiency and explicit control. Windows is the primary target, and Linux is supported from the same codebase.

Status: pre-alpha. The shared core (WeaveHeart) is complete through its planned phases. The renderer, compiler, and editor are not yet built. See `docs/` for the plan and decisions.

## Quick start

The toolchain is pinned in `rust-toolchain.toml`. Any cargo command installs it through rustup.

```
scripts/check.sh          # Linux and macOS: format, lint, build, test
scripts/check.ps1         # Windows PowerShell: the same checks
```

Measure the heart's tick cost on this machine (release build, best of seven runs):

```
cargo run --release -p heart-bench
```

Run the showcase (complex geometry, key, flood, and accent lights):

```
cargo run --release -p showcase -- --size 1920x1080
cargo run --release -p showcase -- --size 1920x1080 --extra-lights 512 --extra-range 1.2
```

Run the lit cube demo (three spinning cubes, lit from one directional light):

```
cargo run --release -p cube
cargo run --release -p cube -- --frames 600   # exits after 600 frames and prints pass timings
```

Run the Phase 0 triangle:

```
cargo run -p triangle
```

## Linux

Build and test need only a Rust toolchain through rustup. No system libraries are required to compile.

Running the triangle needs three things at run time:

- A Vulkan driver for your GPU. On Debian and Ubuntu, install `mesa-vulkan-drivers` (or your vendor's driver). On Fedora, install `mesa-vulkan-drivers` too.
- The Vulkan loader, `libvulkan1`.
- A display, either X11 or Wayland. A headless session can build and test but cannot open a window.

```
sudo apt install mesa-vulkan-drivers libvulkan1     # Debian and Ubuntu
sudo dnf install mesa-vulkan-drivers vulkan-loader  # Fedora
```

To check the Linux window path without a display, `scripts/linux-smoke.sh` runs the triangle under Xvfb with software Vulkan and saves a screenshot (install `xvfb` and `imagemagick`). A sample is in `docs/screenshots/linux-triangle.png`. This has been verified with Mesa's software driver; real GPU drivers are not yet tested.

Platform detection is automatic. On Linux, `PlatformKey::current()` returns `Linux`, and `PlatformOverride` lists for `linux` apply. Build a Linux package with the same `cargo build --release` used everywhere else.

## Layout

```
crates/weave-heart    the shared core: objects, components, scripts, lifecycle
crates/weave-macros   derive macros for components
examples/triangle     the Phase 0 wgpu triangle
examples/heart-bench  tick-cost benchmark for WeaveHeart
examples/cube         lit cube demo on the renderer (weave-eye)
examples/render-bench renderer benchmarks: dense batching and culling, and shader reload
examples/showcase     complex lobed sphere lit by a key, a flood, and an accent light
crates/weave-eye      renderer (E3.1): backend choice, render graph, lit cubes
docs/                 plan decisions, service reference
scripts/              check scripts for each platform, and the Linux smoke test
```

Renderer benchmarks: `target/release/render-bench --scenario dense --count 10000 --mode batched --cull on --sync on` and `--scenario reload`.

Higher-resolution media (1920x1080) is in `docs/media/`.

Benchmark results are kept in [docs/benchmarks.md](docs/benchmarks.md), with the command and environment for each run.

See [CONTRIBUTING.md](CONTRIBUTING.md) for style rules and the review checklist.
