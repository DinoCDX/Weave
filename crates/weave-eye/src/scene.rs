use glam::{Mat4, Quat, Vec3};
use weave_heart::{ObjectHandle, Runtime, Transform};

use crate::renderer::DrawInstance;

#[derive(Clone, Copy, Debug, PartialEq, weave_macros::Component)]
pub struct Renderable {
    pub color: [f32; 4],
}

pub fn model_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::from_array(transform.scale),
        Quat::from_array(transform.rotation),
        Vec3::from_array(transform.translation),
    )
}

pub fn collect_instances(runtime: &Runtime, out: &mut Vec<DrawInstance>) {
    out.clear();
    for object in runtime.world().objects() {
        if !runtime.is_loaded(object) || !runtime.is_enabled(object) {
            continue;
        }
        push_if_renderable(runtime, object, out);
    }
}

fn push_if_renderable(runtime: &Runtime, object: ObjectHandle, out: &mut Vec<DrawInstance>) {
    let world = runtime.world();
    let Some(color) = world.with_component::<Renderable, _>(object, |r| r.color) else {
        return;
    };
    let Some(transform) = world.transform(object) else {
        return;
    };
    out.push(DrawInstance {
        model: model_matrix(&transform),
        color,
    });
}
