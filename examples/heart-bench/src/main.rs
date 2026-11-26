use std::time::Instant;

use weave_heart::services::Time;
use weave_heart::{NoListener, Runtime, Script, ScriptContext};

#[derive(Default)]
struct Spinner {
    turns: f32,
}

impl Script for Spinner {
    type Services = (Time,);

    fn on_update(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        self.turns += cx.time().delta();
    }
}

fn scene(objects: usize, scripted: usize) -> Runtime {
    let mut rt = Runtime::new();
    rt.register_script("Spinner", Spinner::default).unwrap();
    let mut created = Vec::with_capacity(objects);
    for i in 0..objects {
        let object = rt.spawn(None, &format!("object{i}")).unwrap();
        if i < scripted {
            rt.attach_modular(object, "Spinner").unwrap();
        }
        created.push(object);
    }
    for object in created {
        rt.commands().load(object);
    }
    rt.update(0.0);
    rt
}

fn measure(label: &str, objects: usize, scripted: usize, frames: usize) {
    let mut rt = scene(objects, scripted);
    for _ in 0..10 {
        rt.update(0.016);
    }
    let mut best = f64::MAX;
    for _ in 0..7 {
        let start = Instant::now();
        for _ in 0..frames {
            let _ = rt.tick(0.016, &mut NoListener);
        }
        best = best.min(start.elapsed().as_secs_f64() * 1e6 / frames as f64);
    }
    let per_frame = best;
    let per_object_ns = per_frame * 1000.0 / objects as f64;
    println!(
        "{label:<28} best of 7; objects={objects:<7} scripted={scripted:<7} {per_frame:>10.1} us/frame {per_object_ns:>8.1} ns/object"
    );
}

fn main() {
    let frames = 200;
    measure("plain objects", 10_000, 0, frames);
    measure("half scripted", 10_000, 5_000, frames);
    measure("all scripted", 10_000, 10_000, frames);
    measure("plain objects, large", 100_000, 0, 50);
    let _ = (NoListener,);
}
