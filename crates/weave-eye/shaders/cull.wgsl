struct Instance {
    c0: vec4<f32>,
    c1: vec4<f32>,
    c2: vec4<f32>,
    color: u32,
    material: u32,
    pad0: u32,
    pad1: u32,
}

struct Params {
    planes: array<vec4<f32>, 6>,
    first: u32,
    count: u32,
    lod_count: u32,
    phase: u32,
    capacity: u32,
    radius: f32,
    pad0: u32,
    shadow_scale: f32,
}

struct Camera {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    screen: vec4<f32>,
    lods: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> raw: array<Instance>;
@group(0) @binding(2) var<storage, read_write> out: array<Instance>;
@group(0) @binding(3) var<storage, read_write> args: array<atomic<u32>, 20>;
@group(0) @binding(4) var<storage, read_write> flags: array<u32>;
@group(0) @binding(5) var<uniform> camera: Camera;
@group(0) @binding(6) var hzb: texture_2d<f32>;

fn inside_frustum(center: vec3<f32>, reach: f32) -> bool {
    for (var p = 0u; p < 6u; p++) {
        let plane = params.planes[p];
        if (dot(plane.xyz, center) + plane.w < -reach) {
            return false;
        }
    }
    return true;
}

fn screen_radius(view_center: vec3<f32>, radius: f32) -> f32 {
    let dist = max(-view_center.z, 0.0001);
    return radius * camera.proj[1][1] / dist * 0.5 * camera.screen.y;
}

fn select_shadow_lod(reach: f32) -> u32 {
    let r_px = reach * params.shadow_scale;
    var lod = 0u;
    if (r_px < 2.0 * camera.lods.x) {
        lod = 1u;
    }
    if (r_px < 2.0 * camera.lods.y) {
        lod = 2u;
    }
    if (r_px < 2.0 * camera.lods.z) {
        lod = 3u;
    }
    return min(lod, params.lod_count - 1u);
}

fn select_lod(r_px: f32) -> u32 {
    var lod = 0u;
    if (r_px < camera.lods.x) {
        lod = 1u;
    }
    if (r_px < camera.lods.y) {
        lod = 2u;
    }
    if (r_px < camera.lods.z) {
        lod = 3u;
    }
    return min(lod, params.lod_count - 1u);
}

fn occluded(view_center: vec3<f32>, radius: f32) -> bool {
    let dist = -view_center.z;
    let near = camera.lods.w;
    if (dist - radius <= near) {
        return false;
    }
    let nearest_z = view_center.z + radius;
    let clip_near = camera.proj[2][2] * nearest_z + camera.proj[3][2];
    let nearest_depth = clip_near / (-nearest_z);

    let clip = camera.proj * vec4<f32>(view_center, 1.0);
    let ndc = clip.xy / clip.w;
    let width = camera.screen.x;
    let height = camera.screen.y;
    let r_px = radius * camera.proj[1][1] / dist * 0.5 * height;
    let cx = (ndc.x * 0.5 + 0.5) * width;
    let cy = (0.5 - ndc.y * 0.5) * height;
    if (cx + r_px < 0.0 || cx - r_px > width || cy + r_px < 0.0 || cy - r_px > height) {
        return false;
    }

    let levels = u32(camera.screen.z);
    let extent = max(2.0 * r_px, 1.0);
    let level = min(u32(ceil(log2(extent))), levels - 1u);
    let level_i = i32(level);
    let scale = exp2(f32(level));
    let dims = vec2<i32>(textureDimensions(hzb, level_i));
    let x0 = clamp(i32(floor((cx - r_px) / scale)), 0, dims.x - 1);
    let x1 = clamp(i32(floor((cx + r_px) / scale)), 0, dims.x - 1);
    let y0 = clamp(i32(floor((cy - r_px) / scale)), 0, dims.y - 1);
    let y1 = clamp(i32(floor((cy + r_px) / scale)), 0, dims.y - 1);
    let far_depth = max(
        max(textureLoad(hzb, vec2<i32>(x0, y0), level_i).r, textureLoad(hzb, vec2<i32>(x1, y0), level_i).r),
        max(textureLoad(hzb, vec2<i32>(x0, y1), level_i).r, textureLoad(hzb, vec2<i32>(x1, y1), level_i).r),
    );
    return nearest_depth > far_depth;
}

@compute @workgroup_size(64)
fn cull_main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.count) {
        return;
    }
    let index = params.first + id.x;
    if (params.phase == 2u && flags[index] != 0u) {
        return;
    }
    let instance = raw[index];
    let center = vec3<f32>(instance.c0.w, instance.c1.w, instance.c2.w);
    let scale = max(length(instance.c0.xyz), max(length(instance.c1.xyz), length(instance.c2.xyz)));
    let reach = params.radius * scale;
    if (!inside_frustum(center, reach)) {
        if (params.phase == 1u) {
            flags[index] = 0u;
        }
        return;
    }
    let view_center = (camera.view * vec4<f32>(center, 1.0)).xyz;
    if ((params.phase == 1u || params.phase == 2u) && camera.screen.w > 0.5 && occluded(view_center, reach)) {
        if (params.phase == 1u) {
            flags[index] = 0u;
        }
        return;
    }
    let lod = select(select_lod(screen_radius(view_center, reach)), select_shadow_lod(reach), params.phase == 3u);
    let slot = atomicAdd(&args[lod * 5u + 1u], 1u);
    out[lod * params.capacity + params.first + slot] = instance;
    if (params.phase == 1u) {
        flags[index] = 1u;
    }
}
