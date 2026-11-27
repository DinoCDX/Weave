use glam::Vec3;

use crate::mesh::Mesh;

pub(crate) const CLUSTER_TRIANGLES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cluster {
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
    pub(crate) first_index: u32,
    pub(crate) index_count: u32,
}

pub(crate) fn build_clusters(mesh: &Mesh, triangles_per_cluster: usize) -> Vec<Cluster> {
    let per_cluster = triangles_per_cluster.max(1);
    let triangles = mesh.indices.len() / 3;
    (0..triangles)
        .step_by(per_cluster)
        .map(|start| {
            let end = (start + per_cluster).min(triangles);
            let corners = &mesh.indices[start * 3..end * 3];
            let (min, max) = corners.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), index| {
                    let point = Vec3::from(mesh.vertices[*index as usize].position);
                    (min.min(point), max.max(point))
                },
            );
            let center = (min + max) * 0.5;
            let radius = corners
                .iter()
                .map(|index| center.distance(Vec3::from(mesh.vertices[*index as usize].position)))
                .fold(0.0, f32::max);
            Cluster {
                center,
                radius,
                first_index: u32::try_from(start * 3).unwrap_or(u32::MAX),
                index_count: u32::try_from((end - start) * 3).unwrap_or(u32::MAX),
            }
        })
        .collect()
}

pub(crate) fn cluster_bytes(clusters: &[Cluster]) -> Vec<u8> {
    let mut out = Vec::with_capacity(clusters.len() * 32);
    for cluster in clusters {
        for value in [
            cluster.center.x,
            cluster.center.y,
            cluster.center.z,
            cluster.radius,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&cluster.first_index.to_le_bytes());
        out.extend_from_slice(&cluster.index_count.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{cube, lobed_sphere};

    #[test]
    fn clusters_cover_every_index_exactly_once_in_order() {
        for mesh in [cube(), lobed_sphere(32, 16)] {
            let clusters = build_clusters(&mesh, CLUSTER_TRIANGLES);
            let mut next = 0u32;
            for cluster in &clusters {
                assert_eq!(cluster.first_index, next);
                assert!(cluster.index_count > 0 && cluster.index_count % 3 == 0);
                next += cluster.index_count;
            }
            assert_eq!(next as usize, mesh.indices.len());
            assert_eq!(
                clusters.len(),
                mesh.triangle_count().div_ceil(CLUSTER_TRIANGLES)
            );
        }
    }

    #[test]
    fn each_cluster_sphere_contains_its_triangles() {
        let mesh = lobed_sphere(32, 16);
        for cluster in build_clusters(&mesh, CLUSTER_TRIANGLES) {
            let first = cluster.first_index as usize;
            let last = first + cluster.index_count as usize;
            for index in &mesh.indices[first..last] {
                let point = Vec3::from(mesh.vertices[*index as usize].position);
                assert!(cluster.center.distance(point) <= cluster.radius + 1e-4);
            }
        }
    }

    #[test]
    fn a_small_mesh_is_one_cluster() {
        let clusters = build_clusters(&cube(), CLUSTER_TRIANGLES);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].index_count, 36);
    }

    #[test]
    fn cluster_records_are_32_bytes() {
        let clusters = build_clusters(&cube(), CLUSTER_TRIANGLES);
        assert_eq!(cluster_bytes(&clusters).len(), 32);
    }
}
