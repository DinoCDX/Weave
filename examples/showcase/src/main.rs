use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Quat, Vec3};
use weave_eye::{
    Backend, Batch, CUBE, Camera, DrawInstance, FrameParams, Light, MeshHandle, Renderer, ground,
    lobed_sphere,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct Options {
    size: (u32, u32),
    frames: Option<u64>,
    extra_lights: usize,
    extra_range: f32,
    path: weave_eye::LightPath,
    shadows: bool,
    occlusion: bool,
    still: bool,
}

fn parse_options() -> Options {
    let mut options = Options {
        size: (1280, 720),
        frames: None,
        extra_lights: 0,
        extra_range: 6.0,
        path: weave_eye::LightPath::Auto,
        shadows: true,
        occlusion: true,
        still: false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--size" => {
                if let Some((w, h)) = args[i + 1].split_once('x')
                    && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
                {
                    options.size = (w, h);
                }
            }
            "--frames" => options.frames = args[i + 1].parse().ok(),
            "--extra-lights" => options.extra_lights = args[i + 1].parse().unwrap_or(0),
            "--shadows" => options.shadows = args[i + 1] != "off",
            "--occlusion" => options.occlusion = args[i + 1] != "off",
            "--still" => options.still = args[i + 1] == "on",
            "--extra-range" => options.extra_range = args[i + 1].parse().unwrap_or(6.0),
            "--light-path" => {
                options.path = match args[i + 1].as_str() {
                    "direct" => weave_eye::LightPath::Direct,
                    "clustered" => weave_eye::LightPath::Clustered,
                    _ => weave_eye::LightPath::Auto,
                }
            }
            _ => {}
        }
        i += 2;
    }
    options
}

fn lights(extra: usize, extra_range: f32) -> Vec<Light> {
    let mut lights = vec![
        Light::key(Vec3::new(-0.5, -1.0, -0.4), Vec3::new(1.0, 0.96, 0.9), 1.1),
        Light::flood(
            Vec3::new(0.0, 7.0, 6.0),
            Vec3::ZERO,
            Vec3::new(0.85, 0.9, 1.0),
            22.0,
            30.0,
        ),
        Light::accent(
            Vec3::new(-2.6, 1.2, 1.5),
            Vec3::new(0.2, 0.9, 0.8),
            12.0,
            7.0,
        ),
    ];
    for i in 0..extra {
        let angle = i as f32 * 2.399_963;
        let radius = 4.0 + (i % 5) as f32;
        let hue = i as f32 * 0.37;
        lights.push(Light::accent(
            Vec3::new(angle.cos() * radius, 0.5, angle.sin() * radius),
            Vec3::new(
                0.5 + 0.5 * hue.sin(),
                0.5 + 0.5 * (hue + 2.0).sin(),
                0.5 + 0.5 * (hue + 4.0).sin(),
            ),
            4.0,
            extra_range,
        ));
    }
    lights
}

struct Scene {
    sphere: MeshHandle,
    ground: MeshHandle,
    cubes: Vec<DrawInstance>,
    ground_instances: [DrawInstance; 1],
}

struct App {
    options: Options,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    scene: Option<Scene>,
    lights: Vec<Light>,
    camera: Camera,
    params: FrameParams,
    started: Instant,
    frames: u64,
    stats_last: Option<weave_eye::FrameStats>,
}

impl App {
    fn new(options: Options) -> Self {
        let lights = lights(options.extra_lights, options.extra_range);
        Self {
            options,
            window: None,
            renderer: None,
            scene: None,
            lights,
            camera: Camera {
                eye: Vec3::new(0.0, 2.6, 7.5),
                target: Vec3::new(0.0, 0.4, 0.0),
                ..Camera::default()
            },
            params: FrameParams::default(),
            started: Instant::now(),
            frames: 0,
            stats_last: None,
        }
    }

    fn frame(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(scene) = self.scene.as_mut() else {
            return;
        };
        let angle = if self.options.still {
            0.6
        } else {
            self.started.elapsed().as_secs_f32() * 0.4
        };
        let sphere = [DrawInstance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(1.6),
                Quat::from_rotation_y(angle),
                Vec3::new(0.0, 0.2, 0.0),
            ),
            [0.78, 0.62, 0.45, 1.0],
        )
        .with_material(0.9, 0.22)];
        scene.ground_instances[0] = DrawInstance::new(Mat4::IDENTITY, [0.36, 0.37, 0.42, 1.0]);
        let batches = [
            Batch {
                mesh: scene.ground,
                instances: &scene.ground_instances,
            },
            Batch {
                mesh: scene.sphere,
                instances: &sphere,
            },
            Batch {
                mesh: CUBE,
                instances: &scene.cubes,
            },
        ];
        match renderer.draw(&self.camera, &self.lights, &self.params, &batches) {
            Ok(stats) => self.stats_last = Some(stats),
            Err(error) => eprintln!("{error}"),
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
                        .with_title("WeaveEngineX showcase")
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            self.options.size.0,
                            self.options.size.1,
                        )),
                )
                .expect("window"),
        );
        let size = window.inner_size();
        let mut renderer = Renderer::new(window.clone(), size.width, size.height, Backend::Auto)
            .unwrap_or_else(|error| panic!("{error}"));
        renderer.set_culling(true);
        renderer.set_light_path(self.options.path);
        renderer.set_shadows(self.options.shadows);
        renderer.set_occlusion_culling(self.options.occlusion);
        let sphere = renderer.add_mesh_lods(&[
            lobed_sphere(256, 128),
            lobed_sphere(128, 64),
            lobed_sphere(48, 24),
            lobed_sphere(16, 8),
        ]);
        let ground = renderer.add_mesh(&ground(30.0, -1.4));
        let cubes = (0..6)
            .map(|i| {
                let angle = i as f32 / 6.0 * std::f32::consts::TAU;
                DrawInstance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.6),
                        Quat::from_rotation_y(angle),
                        Vec3::new(angle.cos() * 3.2, -0.7, angle.sin() * 3.2),
                    ),
                    [0.3 + 0.1 * i as f32, 0.45, 0.8 - 0.1 * i as f32, 1.0],
                )
            })
            .collect();
        self.scene = Some(Scene {
            sphere,
            ground,
            cubes,
            ground_instances: [DrawInstance::new(Mat4::IDENTITY, [0.5; 4])],
        });
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.started = Instant::now();
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
            WindowEvent::RedrawRequested => {
                self.frame();
                if self
                    .options
                    .frames
                    .is_some_and(|limit| self.frames >= limit)
                {
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
                "showcase: {} frames, {} lights ({} dropped)",
                self.frames,
                self.stats_last.map_or(0, |s| s.lights),
                self.stats_last.map_or(0, |s| s.lights_dropped),
            );
            println!("{}", renderer.graph_report());
        }
    }
}

fn main() {
    let options = parse_options();
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop failed");
}
