use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounding_radius: f32,
}

impl Mesh {
    pub(crate) fn finish(vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        let bounding_radius = vertices
            .iter()
            .map(|v| Vec3::from_array(v.position).length())
            .fold(0.0_f32, f32::max);
        Self {
            vertices,
            indices,
            bounding_radius,
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

pub fn cube() -> Mesh {
    const FACES: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
                [-0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (face, (normal, corners)) in FACES.iter().enumerate() {
        let base = u32::try_from(face * 4).expect("cube indices fit");
        for corner in corners {
            vertices.push(Vertex {
                position: *corner,
                normal: *normal,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::finish(vertices, indices)
}

pub fn ground(half_size: f32, height: f32) -> Mesh {
    let s = half_size;
    let vertices = vec![
        Vertex {
            position: [-s, height, -s],
            normal: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [s, height, -s],
            normal: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [s, height, s],
            normal: [0.0, 1.0, 0.0],
        },
        Vertex {
            position: [-s, height, s],
            normal: [0.0, 1.0, 0.0],
        },
    ];
    Mesh::finish(vertices, vec![0, 2, 1, 0, 3, 2])
}

fn surface(theta: f32, phi: f32) -> Vec3 {
    let direction = Vec3::new(
        theta.sin() * phi.cos(),
        theta.cos(),
        theta.sin() * phi.sin(),
    );
    let lobes = 1.0
        + 0.14 * (5.0 * phi).sin() * (3.0 * theta).sin()
        + 0.07 * (9.0 * phi + 4.0 * theta).cos() * (2.0 * theta).sin()
        + 0.04 * (13.0 * phi - 3.0 * theta).sin();
    direction * lobes
}

pub fn lobed_sphere(segments_around: u32, segments_down: u32) -> Mesh {
    const EPSILON: f32 = 1e-3;
    const H: f32 = 1e-3;
    let rows = segments_down.max(3);
    let columns = segments_around.max(3);
    let mut vertices = Vec::with_capacity(((rows + 1) * (columns + 1)) as usize);
    for row in 0..=rows {
        let theta = EPSILON + (std::f32::consts::PI - 2.0 * EPSILON) * row as f32 / rows as f32;
        for column in 0..=columns {
            let phi = std::f32::consts::TAU * column as f32 / columns as f32;
            let p = surface(theta, phi);
            let dp_dphi = (surface(theta, phi + H) - surface(theta, phi - H)) / (2.0 * H);
            let dp_dtheta = (surface(theta + H, phi) - surface(theta - H, phi)) / (2.0 * H);
            let mut normal = dp_dphi.cross(dp_dtheta).normalize_or_zero();
            if normal.dot(p) < 0.0 {
                normal = -normal;
            }
            vertices.push(Vertex {
                position: p.to_array(),
                normal: normal.to_array(),
            });
        }
    }
    let stride = columns + 1;
    let mut indices = Vec::with_capacity((rows * columns * 6) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let a = row * stride + column;
            let b = a + stride;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh::finish(vertices, indices)
}
