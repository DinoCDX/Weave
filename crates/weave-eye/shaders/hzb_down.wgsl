@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<r32float, write>;

fn tap(p: vec2<i32>, src_size: vec2<i32>) -> f32 {
    return textureLoad(src, min(p, src_size - vec2<i32>(1, 1)), 0).r;
}

@compute @workgroup_size(8, 8)
fn hzb_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(dst);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let src_size = vec2<i32>(textureDimensions(src));
    let base = vec2<i32>(id.xy) * 2;
    let m = max(
        max(tap(base, src_size), tap(base + vec2<i32>(1, 0), src_size)),
        max(tap(base + vec2<i32>(0, 1), src_size), tap(base + vec2<i32>(1, 1), src_size)),
    );
    textureStore(dst, vec2<i32>(id.xy), vec4<f32>(m, 0.0, 0.0, 0.0));
}
