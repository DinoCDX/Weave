use glam::{Mat4, Vec3, Vec4};

use crate::lights::Light;

pub const TILES_X: u32 = 16;
pub const TILES_Y: u32 = 9;
pub const SLICES: u32 = 16;
pub const CLUSTER_COUNT: usize = (TILES_X * TILES_Y * SLICES) as usize;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub near: f32,
    pub far: f32,
}

pub fn slice_of(depth: f32, clip: Clip) -> u32 {
    let depth = depth.max(clip.near);
    let t = (depth / clip.near).ln() / (clip.far / clip.near).ln();
    let index = (t * SLICES as f32).floor();
    index.clamp(0.0, (SLICES - 1) as f32) as u32
}

#[derive(Default)]
pub struct ClusterData {
    pub ranges: Vec<[u32; 2]>,
    pub indices: Vec<u32>,
    bounds: Vec<Option<[u32; 6]>>,
}

impl ClusterData {
    pub fn cluster_of(tile_x: u32, tile_y: u32, slice: u32) -> usize {
        ((slice * TILES_Y + tile_y) * TILES_X + tile_x) as usize
    }

    #[cfg(test)]
    pub fn light_ids(&self, cluster: usize) -> &[u32] {
        let [offset, count] = self.ranges[cluster];
        &self.indices[offset as usize..(offset + count) as usize]
    }
}

fn tile_range(ndc_min: f32, ndc_max: f32, tiles: u32, flip: bool) -> Option<(u32, u32)> {
    if ndc_max < -1.0 || ndc_min > 1.0 {
        return None;
    }
    let to_tile = |ndc: f32| {
        let unit = if flip {
            (1.0 - ndc) * 0.5
        } else {
            (ndc + 1.0) * 0.5
        };
        ((unit * tiles as f32).floor()).clamp(0.0, (tiles - 1) as f32) as u32
    };
    let a = to_tile(ndc_min);
    let b = to_tile(ndc_max);
    Some((a.min(b), a.max(b)))
}

pub fn build(clip: Clip, view: Mat4, projection: Mat4, lights: &[Light], out: &mut ClusterData) {
    out.bounds.clear();
    out.ranges.clear();
    out.ranges.resize(CLUSTER_COUNT, [0, 0]);

    for light in lights {
        let bounds = light_bounds(clip, view, projection, light);
        out.bounds.push(bounds);
        let Some(region) = bounds else {
            continue;
        };
        let (center, range) = view_sphere(view, light);
        for_each_cluster(region, center, range, clip, projection, |cluster| {
            out.ranges[cluster][1] += 1;
        });
    }

    let mut total = 0u32;
    for range in &mut out.ranges {
        range[0] = total;
        total += range[1];
        range[1] = 0;
    }
    out.indices.clear();
    out.indices.resize(total as usize, 0);

    for (light_index, (light, bounds)) in lights.iter().zip(out.bounds.iter()).enumerate() {
        let index = u32::try_from(light_index).unwrap_or(u32::MAX);
        let Some(region) = *bounds else {
            continue;
        };
        let (center, range) = view_sphere(view, light);
        let ranges = &mut out.ranges;
        let indices = &mut out.indices;
        for_each_cluster(region, center, range, clip, projection, |cluster| {
            let slot = ranges[cluster][0] + ranges[cluster][1];
            indices[slot as usize] = index;
            ranges[cluster][1] += 1;
        });
    }
}

fn view_sphere(view: Mat4, light: &Light) -> (Vec3, f32) {
    let (position, range) = match *light {
        Light::Point {
            position, range, ..
        }
        | Light::Spot {
            position, range, ..
        } => (position, range),
        Light::Directional { .. } => (Vec3::ZERO, 0.0),
    };
    (view.transform_point3(position), range)
}

fn for_each_cluster(
    region: [u32; 6],
    center: Vec3,
    radius: f32,
    clip: Clip,
    projection: Mat4,
    mut visit: impl FnMut(usize),
) {
    let [s0, s1, x0, x1, y0, y1] = region;
    for slice in s0..=s1 {
        let (depth_min, depth_max) = slice_depths(slice, clip);
        for tile_y in y0..=y1 {
            for tile_x in x0..=x1 {
                let ndc = tile_ndc(tile_x, tile_y);
                if sphere_touches_frustum_cell(
                    center, radius, depth_min, depth_max, ndc, projection,
                ) {
                    visit(ClusterData::cluster_of(tile_x, tile_y, slice));
                }
            }
        }
    }
}

fn slice_depths(slice: u32, clip: Clip) -> (f32, f32) {
    let ratio = clip.far / clip.near;
    let low = clip.near * ratio.powf(slice as f32 / SLICES as f32);
    let high = clip.near * ratio.powf((slice + 1) as f32 / SLICES as f32);
    (low, high)
}

fn tile_ndc(tile_x: u32, tile_y: u32) -> [f32; 4] {
    let x0 = tile_x as f32 / TILES_X as f32 * 2.0 - 1.0;
    let x1 = (tile_x + 1) as f32 / TILES_X as f32 * 2.0 - 1.0;
    let y_top = 1.0 - tile_y as f32 / TILES_Y as f32 * 2.0;
    let y_bottom = 1.0 - (tile_y + 1) as f32 / TILES_Y as f32 * 2.0;
    [x0, x1, y_bottom, y_top]
}

fn sphere_touches_frustum_cell(
    center: Vec3,
    radius: f32,
    depth_min: f32,
    depth_max: f32,
    ndc: [f32; 4],
    projection: Mat4,
) -> bool {
    let p00 = projection.x_axis.x;
    let p11 = projection.y_axis.y;
    let depth = -center.z;
    if depth + radius < depth_min || depth - radius > depth_max {
        return false;
    }
    let [x0, x1, y0, y1] = ndc;
    let outside =
        |normal: Vec3, offset: f32| (normal.dot(center) + offset) / normal.length() < -radius;
    let ax = Vec3::new(p00, 0.0, 0.0);
    let ay = Vec3::new(0.0, p11, 0.0);
    if outside(ax + Vec3::new(0.0, 0.0, x0), 0.0) {
        return false;
    }
    if outside(-ax - Vec3::new(0.0, 0.0, x1), 0.0) {
        return false;
    }
    if outside(ay + Vec3::new(0.0, 0.0, y0), 0.0) {
        return false;
    }
    if outside(-ay - Vec3::new(0.0, 0.0, y1), 0.0) {
        return false;
    }
    true
}

fn light_bounds(clip: Clip, view: Mat4, projection: Mat4, light: &Light) -> Option<[u32; 6]> {
    let (position, range) = match *light {
        Light::Directional { .. } => return None,
        Light::Point {
            position, range, ..
        }
        | Light::Spot {
            position, range, ..
        } => (position, range),
    };
    let view_pos = view.transform_point3(position);
    let depth = -view_pos.z;
    if depth + range < clip.near {
        return None;
    }
    let s0 = slice_of((depth - range).max(clip.near), clip);
    let s1 = slice_of(depth + range, clip);
    if depth - range <= clip.near {
        return Some([s0, s1, 0, TILES_X - 1, 0, TILES_Y - 1]);
    }
    let nearest = depth - range;
    let clip_pos = projection * Vec4::new(view_pos.x, view_pos.y, view_pos.z, 1.0);
    let ndc = Vec3::new(clip_pos.x, clip_pos.y, clip_pos.z) / clip_pos.w;
    let lateral = (depth * depth + view_pos.x * view_pos.x).sqrt();
    let lateral_y = (depth * depth + view_pos.y * view_pos.y).sqrt();
    let rx = range * projection.x_axis.x * lateral / (depth * nearest);
    let ry = range * projection.y_axis.y * lateral_y / (depth * nearest);
    let (x0, x1) = tile_range(ndc.x - rx, ndc.x + rx, TILES_X, false)?;
    let (y0, y1) = tile_range(ndc.y - ry, ndc.y + ry, TILES_Y, true)?;
    Some([s0, s1, x0, x1, y0, y1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    fn setup() -> (Clip, Mat4, Mat4) {
        let camera = Camera::default();
        (
            Clip {
                near: camera.near,
                far: camera.far,
            },
            camera.view(),
            camera.projection(16.0 / 9.0),
        )
    }

    fn cluster_for_point(view: Mat4, projection: Mat4, clip: Clip, point: Vec3) -> Option<usize> {
        let v = view.transform_point3(point);
        let depth = -v.z;
        if depth < clip.near {
            return None;
        }
        let c = projection * v.extend(1.0);
        let ndc = c.truncate() / c.w;
        if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
            return None;
        }
        let tx = (((ndc.x + 1.0) * 0.5 * TILES_X as f32).floor() as i64)
            .clamp(0, i64::from(TILES_X) - 1) as u32;
        let ty = (((1.0 - ndc.y) * 0.5 * TILES_Y as f32).floor() as i64)
            .clamp(0, i64::from(TILES_Y) - 1) as u32;
        Some(ClusterData::cluster_of(tx, ty, slice_of(depth, clip)))
    }

    fn scattered_lights(count: usize) -> Vec<Light> {
        let mut seed = 0x9E37_79B9_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f32 / 10_000.0
        };
        (0..count)
            .map(|_| Light::Point {
                position: Vec3::new(
                    next() * 12.0 - 6.0,
                    next() * 5.0 - 2.0,
                    next() * 16.0 - 12.0,
                ),
                color: Vec3::ONE,
                intensity: 1.0,
                range: 0.5 + next() * 2.5,
            })
            .collect()
    }

    fn range_of(light: &Light) -> (Vec3, f32) {
        match *light {
            Light::Point {
                position, range, ..
            }
            | Light::Spot {
                position, range, ..
            } => (position, range),
            Light::Directional { .. } => unreachable!("directional lights are not clustered"),
        }
    }

    #[test]
    fn every_visible_point_a_light_reaches_finds_that_light_in_its_cluster() {
        let (clip, view, projection) = setup();
        let lights = scattered_lights(300);
        let mut data = ClusterData::default();
        build(clip, view, projection, &lights, &mut data);

        let mut checked = 0usize;
        for (index, light) in lights.iter().enumerate() {
            let (center, range) = range_of(light);
            for step in 0..24 {
                for ring in 0..12 {
                    let theta = step as f32 / 24.0 * std::f32::consts::TAU;
                    let phi = (ring as f32 + 0.5) / 12.0 * std::f32::consts::PI;
                    let dir =
                        Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
                    let point = center + dir * range * 0.95;
                    let Some(cluster) = cluster_for_point(view, projection, clip, point) else {
                        continue;
                    };
                    checked += 1;
                    assert!(
                        data.light_ids(cluster)
                            .contains(&u32::try_from(index).unwrap()),
                        "light {index} reaches {point:?} but is not in cluster {cluster}"
                    );
                }
            }
        }
        assert!(
            checked > 1000,
            "enough visible samples were checked: {checked}"
        );
    }

    #[test]
    fn lights_behind_the_camera_or_far_off_screen_are_not_listed() {
        let (clip, view, projection) = setup();
        let lights = [
            Light::accent(Vec3::new(0.0, 0.0, 20.0), Vec3::ONE, 1.0, 1.0),
            Light::accent(Vec3::new(500.0, 0.0, 0.0), Vec3::ONE, 1.0, 1.0),
        ];
        let mut data = ClusterData::default();
        build(clip, view, projection, &lights, &mut data);
        assert!(data.indices.is_empty(), "no cluster lists either light");
    }

    #[test]
    fn the_ranges_partition_the_index_list_exactly() {
        let (clip, view, projection) = setup();
        let lights = scattered_lights(200);
        let mut data = ClusterData::default();
        build(clip, view, projection, &lights, &mut data);

        assert_eq!(data.ranges.len(), CLUSTER_COUNT);
        let mut expected_offset = 0u32;
        for range in &data.ranges {
            assert_eq!(range[0], expected_offset, "ranges are contiguous");
            expected_offset += range[1];
        }
        assert_eq!(expected_offset as usize, data.indices.len());
        assert!(data.indices.iter().all(|&i| (i as usize) < lights.len()));
    }

    #[test]
    fn a_wide_light_is_listed_in_many_clusters_and_a_small_one_in_few() {
        let (clip, view, projection) = setup();
        let wide = Light::flood(Vec3::new(0.0, 6.0, 0.0), Vec3::ZERO, Vec3::ONE, 1.0, 25.0);
        let small = Light::accent(Vec3::new(0.0, 0.0, 0.0), Vec3::ONE, 1.0, 0.3);
        let mut wide_data = ClusterData::default();
        build(clip, view, projection, &[wide], &mut wide_data);
        let mut small_data = ClusterData::default();
        build(clip, view, projection, &[small], &mut small_data);
        assert!(wide_data.indices.len() > small_data.indices.len() * 10);
    }

    #[test]
    fn the_slice_index_is_monotonic_in_depth() {
        let clip = Clip {
            near: 0.1,
            far: 100.0,
        };
        let mut previous = 0;
        for step in 0..200 {
            let depth = 0.1 * (1000.0_f32).powf(step as f32 / 199.0);
            let slice = slice_of(depth, clip);
            assert!(slice >= previous);
            previous = slice;
        }
        assert_eq!(previous, SLICES - 1);
    }
}
