use glam::Vec3;
use weave_eye::{Light, MAX_LIGHTS, Mesh, cube, ground, lobed_sphere};

fn check_unit_normals(mesh: &Mesh) {
    for vertex in &mesh.vertices {
        let normal = Vec3::from_array(vertex.normal);
        assert!(
            (normal.length() - 1.0).abs() < 1e-3,
            "normal {normal:?} is not unit length"
        );
    }
}

fn check_indices(mesh: &Mesh) {
    assert_eq!(mesh.indices.len() % 3, 0, "whole triangles");
    assert!(
        mesh.indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len())
    );
}

#[test]
fn the_lobed_sphere_is_a_complex_mesh_with_smooth_outward_normals() {
    let mesh = lobed_sphere(128, 64);
    check_indices(&mesh);
    check_unit_normals(&mesh);
    assert!(
        mesh.triangle_count() > 15_000,
        "triangles: {}",
        mesh.triangle_count()
    );
    let dense = lobed_sphere(384, 192);
    assert!(
        dense.vertices.len() > usize::from(u16::MAX),
        "needs 32-bit indices"
    );
    check_indices(&dense);
    for vertex in mesh.vertices.iter().step_by(37) {
        let position = Vec3::from_array(vertex.position);
        let normal = Vec3::from_array(vertex.normal);
        assert!(
            normal.dot(position) > 0.0,
            "normal points outward at {position:?}"
        );
    }
}

#[test]
fn the_bounding_radius_bounds_every_vertex() {
    for mesh in [cube(), ground(10.0, -1.0), lobed_sphere(64, 32)] {
        for vertex in &mesh.vertices {
            assert!(Vec3::from_array(vertex.position).length() <= mesh.bounding_radius + 1e-5);
        }
    }
    assert!((cube().bounding_radius - 0.866_025_4).abs() < 1e-4);
}

#[test]
fn the_ground_is_two_triangles_facing_up() {
    let mesh = ground(30.0, -1.4);
    check_indices(&mesh);
    assert_eq!(mesh.triangle_count(), 2);
    assert!(
        mesh.vertices
            .iter()
            .all(|v| v.position[1] == -1.4 && v.normal == [0.0, 1.0, 0.0])
    );
}

#[test]
fn the_light_limit_is_four_thousand_ninety_six() {
    assert_eq!(MAX_LIGHTS, 4096);
}

#[test]
fn the_showcase_lights_are_built_from_helpers_with_the_right_kinds() {
    let key = Light::key(Vec3::new(0.0, -2.0, 0.0), Vec3::ONE, 1.0);
    let Light::Directional { direction, .. } = key else {
        panic!("key light is directional");
    };
    assert!(
        (direction.length() - 1.0).abs() < 1e-6,
        "direction is normalised"
    );

    let flood = Light::flood(Vec3::new(0.0, 7.0, 0.0), Vec3::ZERO, Vec3::ONE, 1.0, 30.0);
    let Light::Spot {
        direction: flood_dir,
        inner_degrees,
        outer_degrees,
        ..
    } = flood
    else {
        panic!("flood is a spot");
    };
    assert!(
        (flood_dir - Vec3::NEG_Y).length() < 1e-5,
        "flood points at the target"
    );
    assert!(outer_degrees > inner_degrees, "flood has a soft edge");
    assert!(outer_degrees >= 60.0, "flood is wide");

    let accent = Light::accent(Vec3::X, Vec3::ONE, 1.0, 5.0);
    assert!(matches!(accent, Light::Point { .. }));
}
