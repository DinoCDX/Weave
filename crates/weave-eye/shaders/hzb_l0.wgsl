@group(0) @binding(0) var depth_in: texture_depth_2d;
@group(0) @binding(1) var hzb_out: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn hzb_l0(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(hzb_out);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let depth = textureLoad(depth_in, vec2<i32>(id.xy), 0);
    textureStore(hzb_out, vec2<i32>(id.xy), vec4<f32>(depth, 0.0, 0.0, 0.0));
}
