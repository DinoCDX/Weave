use glam::{Mat4, Vec3, Vec4};
use weave_eye::{Backend, Camera, DrawInstance, Renderable, collect_instances, cube};
use weave_heart::{PlatformKey, Runtime, Transform};

#[test]
fn each_platform_gets_its_backend() {
    assert_eq!(
        Backend::for_platform(Some(PlatformKey::Windows)),
        Backend::Dx12
    );
    assert_eq!(
        Backend::for_platform(Some(PlatformKey::Linux)),
        Backend::Vulkan
    );
    assert_eq!(
        Backend::for_platform(Some(PlatformKey::Android)),
        Backend::Auto
    );
    assert_eq!(Backend::for_platform(None), Backend::Auto);
}

#[test]
fn the_cube_has_24_vertices_36_indices_and_unit_normals() {
    let mesh = cube();
    let vertices = &mesh.vertices;
    let indices = &mesh.indices;
    assert_eq!(vertices.len(), 24);
    assert_eq!(indices.len(), 36);
    assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));
    for vertex in vertices {
        let normal = Vec3::from_array(vertex.normal);
        assert!((normal.length() - 1.0).abs() < 1e-6);
        assert!(vertex.position.iter().all(|c| c.abs() <= 0.5 + 1e-6));
    }
    assert_eq!(indices.len() % 3, 0);
}

#[test]
fn the_camera_centres_its_target_and_keeps_depth_in_zero_to_one() {
    let camera = Camera::default();
    let clip = camera.view_proj(16.0 / 9.0) * camera.target.extend(1.0);
    let ndc = clip / clip.w;
    assert!(
        ndc.x.abs() < 1e-5,
        "target is centred horizontally: {ndc:?}"
    );
    assert!(ndc.y.abs() < 1e-5, "target is centred vertically: {ndc:?}");
    assert!((0.0..=1.0).contains(&ndc.z), "depth is in [0, 1]: {ndc:?}");
}

#[test]
fn the_view_projection_is_finite_for_a_range_of_aspect_ratios() {
    let camera = Camera::default();
    for aspect in [0.5_f32, 1.0, 4.0 / 3.0, 21.0 / 9.0] {
        let m: Mat4 = camera.view_proj(aspect);
        assert!(m.to_cols_array().iter().all(|v| v.is_finite()));
        assert_ne!(m * Vec4::new(0.0, 0.0, 0.0, 1.0), Vec4::ZERO);
    }
}

#[test]
fn only_loaded_enabled_renderable_objects_are_collected() {
    let mut rt = Runtime::new();
    let shown = rt.spawn(None, "shown").unwrap();
    let hidden = rt.spawn(None, "hidden").unwrap();
    let unloaded = rt.spawn(None, "unloaded").unwrap();
    let plain = rt.spawn(None, "plain").unwrap();
    for object in [shown, hidden, plain] {
        rt.commands().load(object);
    }
    rt.insert_component(
        shown,
        Renderable {
            color: [1.0, 0.0, 0.0, 1.0],
        },
    );
    rt.insert_component(
        hidden,
        Renderable {
            color: [0.0, 1.0, 0.0, 1.0],
        },
    );
    rt.insert_component(
        unloaded,
        Renderable {
            color: [0.0, 0.0, 1.0, 1.0],
        },
    );
    rt.commands().set_transform(
        shown,
        Transform {
            translation: [2.0, 3.0, 4.0],
            ..Transform::default()
        },
    );
    rt.update(0.0);
    rt.commands().set_enabled(hidden, false);
    rt.update(0.0);

    let mut out: Vec<DrawInstance> = Vec::new();
    collect_instances(&rt, &mut out);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].color, [1.0, 0.0, 0.0, 1.0]);
    let translation = out[0].model.col(3);
    assert_eq!(translation, Vec4::new(2.0, 3.0, 4.0, 1.0));
    assert!(!rt.is_loaded(unloaded));
}

#[test]
fn collecting_reuses_the_callers_buffer() {
    let mut rt = Runtime::new();
    let object = rt.spawn(None, "one").unwrap();
    rt.insert_component(object, Renderable { color: [1.0; 4] });
    rt.commands().load(object);
    rt.update(0.0);
    let mut out = Vec::with_capacity(8);
    collect_instances(&rt, &mut out);
    collect_instances(&rt, &mut out);
    assert_eq!(out.len(), 1);
    assert!(out.capacity() >= 8);
}
