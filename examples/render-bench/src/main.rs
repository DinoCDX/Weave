use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{Mat4, Quat, Vec3};
use weave_eye::{
    Backend, Camera, DirectionalLight, DrawInstance, DrawMode, FrameParams, FrameStats, Renderer,
    ShaderUpdate, Timing,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const EMBEDDED: &str = include_str!("../../../crates/weave-eye/shaders/cube.wgsl");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scenario {
    Dense,
    Reload,
}

struct Options {
    scenario: Scenario,
    count: usize,
    mode: DrawMode,
    culling: bool,
    frames: u64,
    sync: bool,
}

fn parse_options() -> Options {
    let mut options = Options {
        scenario: Scenario::Dense,
        count: 10_000,
        mode: DrawMode::Batched,
        culling: false,
        frames: 300,
        sync: false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).cloned().unwrap_or_default();
        match args[i].as_str() {
            "--scenario" => {
                options.scenario = if value == "reload" {
                    Scenario::Reload
                } else {
                    Scenario::Dense
                }
            }
            "--count" => options.count = value.parse().unwrap_or(options.count),
            "--mode" => {
                options.mode = if value == "unbatched" {
                    DrawMode::Unbatched
                } else {
                    DrawMode::Batched
                }
            }
            "--cull" => options.culling = value == "on",
            "--frames" => options.frames = value.parse().unwrap_or(options.frames),
            "--sync" => {
                options.sync = value == "on";
                i += 2;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    options
}

fn dense_instances(count: usize) -> Vec<DrawInstance> {
    let side = (count as f32).sqrt().ceil() as usize;
    let half = side as f32 / 2.0;
    (0..count)
        .map(|i| {
            let x = ((i % side) as f32 - half) * 1.2;
            let z = ((i / side) as f32 - half) * 1.2;
            let shade = 0.3 + 0.7 * ((i % 7) as f32 / 6.0);
            DrawInstance {
                model: Mat4::from_scale_rotation_translation(
                    Vec3::splat(0.5),
                    Quat::IDENTITY,
                    Vec3::new(x, 0.0, z),
                ),
                color: [shade, 0.5, 1.0 - shade, 1.0],
            }
        })
        .collect()
}

fn dense_camera() -> Camera {
    Camera {
        eye: Vec3::new(0.0, 12.0, 70.0),
        target: Vec3::ZERO,
        ..Camera::default()
    }
}

fn delta(now: Option<Timing>, before: Option<Timing>) -> Duration {
    match (now, before) {
        (Some(now), Some(before)) if now.calls > before.calls => {
            (now.total - before.total) / u32::try_from(now.calls - before.calls).unwrap_or(1)
        }
        _ => Duration::ZERO,
    }
}

struct Dense {
    options: Options,
    instances: Vec<DrawInstance>,
    frame: u64,
    warm: Option<(Option<Timing>, Option<Timing>)>,
    cull_sum: Duration,
    frame_sum: Duration,
    submitted: usize,
    drawn_sum: u64,
    measured: u64,
}

struct Reload {
    path: PathBuf,
    frame: u64,
    edit_written: Option<Instant>,
    latency: Option<Duration>,
    broken_error: Option<String>,
    broken_kept_drawing: bool,
    restored: Option<ShaderUpdate>,
    edited_text: String,
}

enum Job {
    Dense(Dense),
    Reload(Reload),
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    job: Job,
}

const WARMUP: u64 = 30;

impl App {
    fn new(options: Options) -> Self {
        let job = match options.scenario {
            Scenario::Dense => Job::Dense(Dense {
                instances: dense_instances(options.count),
                options,
                frame: 0,
                warm: None,
                cull_sum: Duration::ZERO,
                frame_sum: Duration::ZERO,
                submitted: 0,
                drawn_sum: 0,
                measured: 0,
            }),
            Scenario::Reload => {
                let path = std::env::temp_dir().join("weaveeye-reload-test.wgsl");
                fs::write(&path, EMBEDDED).expect("write the shader file");
                Job::Reload(Reload {
                    path,
                    frame: 0,
                    edit_written: None,
                    latency: None,
                    broken_error: None,
                    broken_kept_drawing: false,
                    restored: None,
                    edited_text: EMBEDDED.replace(
                        "return vec4<f32>(lit, input.color.a);",
                        "return vec4<f32>(lit.b, lit.g, lit.r, input.color.a);",
                    ),
                })
            }
        };
        Self {
            window: None,
            renderer: None,
            job,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("render-bench"))
                .expect("window"),
        );
        let size = window.inner_size();
        let mut renderer = Renderer::new(window.clone(), size.width, size.height, Backend::Vulkan)
            .unwrap_or_else(|error| panic!("{error}"));
        if let Job::Dense(dense) = &self.job {
            renderer.set_draw_mode(dense.options.mode);
            renderer.set_culling(dense.options.culling);
        }
        if let Job::Reload(reload) = &self.job {
            renderer
                .use_shader_file(&reload.path)
                .expect("the initial shader compiles");
        }
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                let finished = match &mut self.job {
                    Job::Dense(dense) => dense_step(dense, renderer),
                    Job::Reload(reload) => reload_step(reload, renderer),
                };
                if finished {
                    report(&self.job, renderer);
                    event_loop.exit();
                    return;
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn dense_step(dense: &mut Dense, renderer: &mut Renderer) -> bool {
    let camera = dense_camera();
    let light = DirectionalLight::default();
    let params = FrameParams::default();
    if dense.frame == WARMUP {
        dense.warm = Some((
            renderer.pass_timing("opaque"),
            renderer.pass_timing("clear"),
        ));
    }
    let started = Instant::now();
    let stats: FrameStats = renderer
        .draw(&camera, &light, &params, &dense.instances)
        .unwrap_or_else(|error| panic!("{error}"));
    if dense.options.sync {
        renderer.wait_idle();
    }
    let elapsed = started.elapsed();
    if dense.frame >= WARMUP {
        dense.frame_sum += elapsed;
        dense.cull_sum += stats.cull;
        dense.submitted += stats.submitted;
        dense.drawn_sum += u64::from(stats.drawn);
        dense.measured += 1;
    }
    dense.frame += 1;
    dense.frame >= dense.options.frames
}

fn reload_step(reload: &mut Reload, renderer: &mut Renderer) -> bool {
    let camera = Camera::default();
    let light = DirectionalLight::default();
    let params = FrameParams::default();
    let frame = reload.frame;

    if frame == 60 {
        fs::write(&reload.path, &reload.edited_text).expect("write the edit");
        reload.edit_written = Some(Instant::now());
    }
    if frame == 120 {
        fs::write(&reload.path, "fn broken( -> vec4<f32> {").expect("write the broken edit");
        reload.edit_written = Some(Instant::now());
    }
    if frame == 180 {
        fs::write(&reload.path, EMBEDDED).expect("restore");
        reload.edit_written = Some(Instant::now());
    }

    let polled = renderer.reload_shader();
    match (frame, polled) {
        (60, Ok(ShaderUpdate::Compiled)) => {
            reload.latency = reload.edit_written.map(|t| t.elapsed());
        }
        (120, Err(error)) => {
            reload.broken_error = Some(error.to_string());
        }
        (180, Ok(update)) => {
            reload.restored = Some(update);
        }
        _ => {}
    }

    let instances = [DrawInstance {
        model: Mat4::IDENTITY,
        color: [1.0, 0.5, 0.2, 1.0],
    }];
    let drew = renderer.draw(&camera, &light, &params, &instances);
    if (120..180).contains(&frame)
        && let Ok(stats) = drew
    {
        reload.broken_kept_drawing = stats.drawn == 1 && renderer.shader_error().is_some();
    }

    reload.frame += 1;
    reload.frame >= 240
}

fn report(job: &Job, renderer: &Renderer) {
    match job {
        Job::Dense(dense) => {
            let (opaque_before, clear_before) = dense.warm.unwrap_or((None, None));
            let opaque = delta(renderer.pass_timing("opaque"), opaque_before);
            let clear = delta(renderer.pass_timing("clear"), clear_before);
            let measured = dense.measured.max(1);
            println!(
                "dense count={} mode={:?} cull={} sync={} frames={} | opaque {:.1} us | clear {:.1} us | cull {:.1} us | frame {:.1} us | submitted {} | drawn {:.0}",
                dense.options.count,
                dense.options.mode,
                if dense.options.culling { "on" } else { "off" },
                if dense.options.sync { "on" } else { "off" },
                dense.options.frames,
                opaque.as_secs_f64() * 1e6,
                clear.as_secs_f64() * 1e6,
                (dense.cull_sum / u32::try_from(measured).unwrap_or(1)).as_secs_f64() * 1e6,
                (dense.frame_sum / u32::try_from(measured).unwrap_or(1)).as_secs_f64() * 1e6,
                dense.submitted / usize::try_from(measured).unwrap_or(1),
                dense.drawn_sum as f64 / measured as f64,
            );
        }
        Job::Reload(reload) => {
            println!(
                "reload: edit visible after {} | broken edit rejected: {} | renderer kept drawing: {} | restored original: {:?}",
                reload
                    .latency
                    .map(|d| format!("{:.1} ms", d.as_secs_f64() * 1e3))
                    .unwrap_or_else(|| "not applied".into()),
                reload.broken_error.is_some(),
                reload.broken_kept_drawing,
                reload.restored,
            );
        }
    }
}

fn main() {
    let options = parse_options();
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop failed");
}
