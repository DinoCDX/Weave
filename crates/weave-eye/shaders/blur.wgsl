@group(0) @binding(0) var source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let pixel = vec2<i32>(position.xy);
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let sample_pixel = clamp(pixel + vec2<i32>(x, y), vec2<i32>(0, 0), size - vec2<i32>(1, 1));
            sum = sum + textureLoad(source, sample_pixel, 0).r;
        }
    }
    let average = sum / 9.0;
    return vec4<f32>(average, average, average, 1.0);
}
