use std::collections::HashMap;
use std::fmt;

use glam::Vec3;

use crate::mesh::{Mesh, Vertex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjError {
    Parse { line: usize, reason: String },
    NoFaces,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { line, reason } => write!(f, "line {line}: {reason}"),
            Self::NoFaces => write!(f, "the file contains no faces"),
        }
    }
}

impl std::error::Error for ObjError {}

type Corner = (usize, Option<usize>);

fn parse_floats(
    parts: &mut std::str::SplitWhitespace<'_>,
    line: usize,
) -> Result<[f32; 3], ObjError> {
    let mut out = [0.0f32; 3];
    for value in &mut out {
        let text = parts.next().ok_or_else(|| ObjError::Parse {
            line,
            reason: "expected three numbers".to_owned(),
        })?;
        *value = text.parse().map_err(|_| ObjError::Parse {
            line,
            reason: format!("{text:?} is not a number"),
        })?;
    }
    Ok(out)
}

fn resolve(index: &str, count: usize, line: usize) -> Result<usize, ObjError> {
    let value: i64 = index.parse().map_err(|_| ObjError::Parse {
        line,
        reason: format!("{index:?} is not an index"),
    })?;
    let resolved = if value > 0 {
        value - 1
    } else if value < 0 {
        count as i64 + value
    } else {
        -1
    };
    if resolved < 0 || resolved as usize >= count {
        return Err(ObjError::Parse {
            line,
            reason: format!("index {value} is outside 1..={count}"),
        });
    }
    Ok(resolved as usize)
}

fn parse_corner(
    text: &str,
    positions: usize,
    normals: usize,
    line: usize,
) -> Result<Corner, ObjError> {
    let mut fields = text.split('/');
    let position = resolve(fields.next().unwrap_or(""), positions, line)?;
    let _texture = fields.next();
    let normal = match fields.next() {
        Some(field) if !field.is_empty() => Some(resolve(field, normals, line)?),
        _ => None,
    };
    Ok((position, normal))
}

pub fn parse_obj(text: &str) -> Result<Mesh, ObjError> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut faces: Vec<Vec<Corner>> = Vec::new();

    for (number, raw) in text.lines().enumerate() {
        let line = number + 1;
        let content = raw.split('#').next().unwrap_or("");
        let mut parts = content.split_whitespace();
        match parts.next() {
            Some("v") => positions.push(parse_floats(&mut parts, line)?),
            Some("vn") => normals.push(parse_floats(&mut parts, line)?),
            Some("f") => {
                let corners = parts
                    .map(|part| parse_corner(part, positions.len(), normals.len(), line))
                    .collect::<Result<Vec<_>, _>>()?;
                if corners.len() < 3 {
                    return Err(ObjError::Parse {
                        line,
                        reason: "a face needs at least three corners".to_owned(),
                    });
                }
                faces.push(corners);
            }
            _ => {}
        }
    }
    if faces.is_empty() {
        return Err(ObjError::NoFaces);
    }

    let computed = if normals.is_empty() {
        Some(smooth_normals(&positions, &faces))
    } else {
        None
    };

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut lookup: HashMap<Corner, u32> = HashMap::new();
    let mut indices: Vec<u32> = Vec::new();
    for face in &faces {
        for k in 1..face.len() - 1 {
            for corner in [face[0], face[k], face[k + 1]] {
                let index = *lookup.entry(corner).or_insert_with(|| {
                    let (position, normal) = corner;
                    let normal = match (normal, &computed) {
                        (Some(n), _) => normals[n],
                        (None, Some(smooth)) => smooth[position],
                        (None, None) => [0.0, 1.0, 0.0],
                    };
                    vertices.push(Vertex {
                        position: positions[position],
                        normal,
                    });
                    (vertices.len() - 1) as u32
                });
                indices.push(index);
            }
        }
    }
    Ok(Mesh::finish(vertices, indices))
}

fn smooth_normals(positions: &[[f32; 3]], faces: &[Vec<Corner>]) -> Vec<[f32; 3]> {
    let mut sums = vec![Vec3::ZERO; positions.len()];
    for face in faces {
        for k in 1..face.len() - 1 {
            let a = Vec3::from_array(positions[face[0].0]);
            let b = Vec3::from_array(positions[face[k].0]);
            let c = Vec3::from_array(positions[face[k + 1].0]);
            let area_weighted = (b - a).cross(c - a);
            for corner in [face[0], face[k], face[k + 1]] {
                sums[corner.0] += area_weighted;
            }
        }
    }
    sums.iter()
        .map(|sum| sum.normalize_or_zero().to_array())
        .collect()
}

impl Mesh {
    pub fn fitted(mut self, radius: f32) -> Self {
        let (min, max) = self.vertices.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), v| {
                let p = Vec3::from_array(v.position);
                (lo.min(p), hi.max(p))
            },
        );
        let center = (min + max) * 0.5;
        let current = self
            .vertices
            .iter()
            .map(|v| (Vec3::from_array(v.position) - center).length())
            .fold(0.0_f32, f32::max);
        let scale = if current > 0.0 { radius / current } else { 1.0 };
        for vertex in &mut self.vertices {
            let p = (Vec3::from_array(vertex.position) - center) * scale;
            vertex.position = p.to_array();
        }
        Mesh::finish(self.vertices, self.indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_are_split_into_two_triangles_and_shared_corners_are_reused() {
        let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvn 0 0 1\nf 1//1 2//1 3//1 4//1\n";
        let mesh = parse_obj(text).unwrap();
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.vertices.len(), 4);
        assert!(mesh.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn negative_indices_count_back_from_the_last_element() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3 -2 -1\n";
        let mesh = parse_obj(text).unwrap();
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[2].position, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn missing_normals_are_computed_and_point_away_from_the_surface_plane() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 0 1\nf 1 3 2\n";
        let mesh = parse_obj(text).unwrap();
        for vertex in &mesh.vertices {
            let n = Vec3::from_array(vertex.normal);
            assert!((n.length() - 1.0).abs() < 1e-5);
            assert!(
                n.y.abs() > 0.99,
                "a flat triangle in XZ has a vertical normal"
            );
        }
    }

    #[test]
    fn malformed_input_is_reported_with_its_line() {
        let error = parse_obj("v 0 0 0\nf 1 9 1\n").unwrap_err();
        assert!(matches!(error, ObjError::Parse { line: 2, .. }));
        assert_eq!(parse_obj("v 0 0 0\n").unwrap_err(), ObjError::NoFaces);
        assert!(matches!(
            parse_obj("v 0 0 x\nf 1 1 1\n").unwrap_err(),
            ObjError::Parse { line: 1, .. }
        ));
    }

    #[test]
    fn fitting_centres_the_model_and_sets_its_bounding_radius() {
        let text = "v 10 10 10\nv 12 10 10\nv 10 12 10\nf 1 2 3\n";
        let mesh = parse_obj(text).unwrap().fitted(1.5);
        assert!((mesh.bounding_radius - 1.5).abs() < 1e-4);
        let mean: Vec3 = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from_array(v.position))
            .sum::<Vec3>()
            / mesh.vertices.len() as f32;
        assert!(mean.length() < 1.5);
    }
}
