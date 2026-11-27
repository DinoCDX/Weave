use std::sync::Arc;
use std::time::Instant;

use glam::Quat;
use weave_eye::{
    Backend, Batch, CUBE, Camera, DrawInstance, FrameParams, Light, Renderable, Renderer,
    collect_instances,
};
use weave_heart::services::Time;
use weave_heart::{PlatformKey, Runtime, Script, ScriptContext, Transform};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct Spin {
    speed: f32,
    angle: f32,
    translation: [f32; 3],
}

impl Script<(Time,)> for Spin {
    fn on_update(&mut self, cx: &mut ScriptContext<'_, (Time,)>) {
        self.angle += self.speed * cx.time().delta();
        let rotation = Quat::from_rotation_y(self.angle) * Quat::from_rotation_x(self.angle * 0.5);
        cx.set_transform(Transform {
            translation: self.translation,
            rotation: rotation.to_array(),
            ..Transform::default()
        });
    }
}

struct App {
    size: (u32, u32),
    max_frames: Option<u64>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    runtime: Runtime,
    instances: Vec<DrawInstance>,
    camera: Camera,
    lights: Vec<Light>,
    params: FrameParams,
    last: Instant,
    frames: u64,
}

impl App {
    fn new(max_frames: Option<u64>, size: (u32, u32)) -> Self {
        let mut runtime = Runtime::new();
        let layout = [
            (-1.6, [0.9, 0.3, 0.25, 1.0], 0.7),
            (0.0, [0.3, 0.85, 0.4, 1.0], 1.1),
            (1.6, [0.3, 0.45, 0.95, 1.0], 1.6),
        ];
        for (index, (x, color, speed)) in layout.into_iter().enumerate() {
            let object = runtime
                .spawn(None, &format!("cube{index}"))
                .expect("unique names");
            runtime.insert_component(object, Renderable { color });
            runtime
                .attach(
                    object,
                    Spin {
                        speed,
                        angle: 0.0,
                        translation: [x, 0.0, 0.0],
                    },
                )
                .expect("object is live");
            runtime.commands().set_transform(
                object,
                Transform {
                    translation: [x, 0.0, 0.0],
                    scale: [0.8; 3],
                    ..Transform::default()
                },
            );
            runtime.commands().load(object);
        }
        Self {
            size,
            max_frames,
            window: None,
            renderer: None,
            runtime,
            instances: Vec::new(),
            camera: Camera::default(),
            lights: vec![
                Light::default(),
                Light::Point {
                    position: glam::Vec3::new(0.0, 1.5, 2.0),
                    color: glam::Vec3::new(1.0, 0.7, 0.4),
                    intensity: 4.0,
                    range: 8.0,
                },
            ],
            params: FrameParams::default(),
            last: Instant::now(),
            frames: 0,
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        let report = self.runtime.update(dt);
        if !report.is_clean() {
            eprintln!("frame {}: {} issue(s)", self.frames, report.issue_count());
        }
        collect_instances(&self.runtime, &mut self.instances);
        if let Some(renderer) = self.renderer.as_mut()
            && let Err(error) = renderer.draw(
                &self.camera,
                &self.lights,
                &self.params,
                &[Batch {
                    mesh: CUBE,
                    instances: &self.instances,
                }],
            )
        {
            eprintln!("{error}");
        }
        self.frames += 1;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("WeaveEngineX cube")
                        .with_inner_size(winit::dpi::PhysicalSize::new(self.size.0, self.size.1)),
                )
                .expect("window"),
        );
        let size = window.inner_size();
        let renderer = Renderer::new(
            window.clone(),
            size.width,
            size.height,
            Backend::for_platform(PlatformKey::current()),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape) =>
            {
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if self.max_frames.is_some_and(|limit| self.frames >= limit) {
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

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(renderer) = self.renderer.as_ref() {
            println!(
                "rendered {} frames\n{}",
                self.frames,
                renderer.graph_report()
            );
        }
    }
}

fn parse_frame_limit() -> Option<u64> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--frames" {
            return args.next().and_then(|value| value.parse().ok());
        }
    }
    None
}

fn parse_size() -> (u32, u32) {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--size"
            && let Some((w, h)) = args
                .next()
                .and_then(|v| v.split_once('x').map(|(w, h)| (w.to_owned(), h.to_owned())))
            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
        {
            return (w, h);
        }
    }
    (800, 600)
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new(parse_frame_limit(), parse_size());
    event_loop.run_app(&mut app).expect("event loop failed");
}
