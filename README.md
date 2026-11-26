# WeaveEngineX

A Rust game engine for 3D games on desktop, built around efficiency and explicit control. Windows is the primary target, and Linux is supported from the same codebase.

Status: pre-alpha. The shared core (WeaveHeart) is complete through its planned phases. The renderer, compiler, and editor are not yet built. See `docs/` for the plan and decisions.

## Quick start

The toolchain is pinned in `rust-toolchain.toml`. Any cargo command installs it through rustup.

```
scripts/check.sh          # Linux and macOS: format, lint, build, test
scripts/check.ps1         # Windows PowerShell: the same checks
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
docs/                 plan decisions, service reference
scripts/              check scripts for each platform, and the Linux smoke test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for style rules and the review checklist.
