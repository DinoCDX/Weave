struct Light {
    position_type: vec4<f32>,
    direction: vec4<f32>,
    color_intensity: vec4<f32>,
    params: vec4<f32>,
}

struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    params: vec4<f32>,
    grid: vec4<f32>,
    viewport: vec4<f32>,
    shadow_view_proj: mat4x4<f32>,
    shadow_params: vec4<f32>,
    directional: array<Light, 4>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<storage, read> light_indices: array<u32>;
@group(0) @binding(3) var<storage, read> cluster_ranges: array<vec2<u32>>;
@group(0) @binding(4) var shadow_map: texture_depth_2d;
@group(0) @binding(5) var shadow_sampler: sampler_comparison;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(7) color: vec4<f32>,
    @location(8) material: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) depth: f32,
    @location(4) material: vec4<f32>,
}

struct Surface {
    n: vec3<f32>,
    v: vec3<f32>,
    world: vec3<f32>,
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32,
}

fn instance_matrix(c0: vec4<f32>, c1: vec4<f32>, c2: vec4<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(
        vec4<f32>(c0.xyz, 0.0),
        vec4<f32>(c1.xyz, 0.0),
        vec4<f32>(c2.xyz, 0.0),
        vec4<f32>(c0.w, c1.w, c2.w, 1.0),
    );
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let model = instance_matrix(v.model_0, v.model_1, v.model_2);
    let world = (model * vec4<f32>(v.position, 1.0)).xyz;
    var out: VertexOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.normal = normalize((model * vec4<f32>(v.normal, 0.0)).xyz);
    out.color = v.color;
    out.world = world;
    out.depth = -(frame.view * vec4<f32>(world, 1.0)).z;
    out.material = v.material;
    return out;
}

fn distribution_ggx(n_dot_h: f32, a2: f32) -> f32 {
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (denom * denom);
}

fn visibility_smith(n_dot_v: f32, n_dot_l: f32, a2: f32) -> f32 {
    let gv = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let gl = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 0.0001);
}

fn brdf(s: Surface, l: vec3<f32>) -> vec3<f32> {
    let h = normalize(s.v + l);
    let n_dot_v = max(dot(s.n, s.v), 0.0001);
    let n_dot_l = max(dot(s.n, l), 0.0001);
    let n_dot_h = max(dot(s.n, h), 0.0);
    let v_dot_h = max(dot(s.v, h), 0.0);
    let roughness = clamp(s.roughness, 0.045, 1.0);
    let a = roughness * roughness;
    let a2 = a * a;
    let f0 = mix(vec3<f32>(0.04), s.albedo, s.metallic);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - v_dot_h, 5.0);
    let specular = distribution_ggx(n_dot_h, a2) * visibility_smith(n_dot_v, n_dot_l, a2) * fresnel;
    let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - s.metallic) * s.albedo;
    return diffuse + specular;
}

fn shade(light: Light, s: Surface) -> vec3<f32> {
    let kind = u32(light.position_type.w);
    var l = vec3<f32>(0.0, 1.0, 0.0);
    var attenuation = 1.0;
    if (kind == 0u) {
        l = -normalize(light.direction.xyz);
    } else {
        let to_light = light.position_type.xyz - s.world;
        let distance = length(to_light);
        l = to_light / max(distance, 0.0001);
        let range = max(light.params.x, 0.0001);
        let window = clamp(1.0 - pow(distance / range, 4.0), 0.0, 1.0);
        attenuation = window * window / (distance * distance + 1.0);
        if (kind == 2u) {
            let along = dot(-l, normalize(light.direction.xyz));
            attenuation = attenuation * smoothstep(light.params.z, light.params.y, along);
        }
    }
    let n_dot_l = max(dot(s.n, l), 0.0);
    if (n_dot_l <= 0.0) {
        return vec3<f32>(0.0);
    }
    let radiance = light.color_intensity.rgb * light.color_intensity.w * attenuation;
    return brdf(s, l) * radiance * n_dot_l;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let numerator = x * (2.51 * x + vec3<f32>(0.03));
    let denominator = x * (2.43 * x + vec3<f32>(0.59)) + vec3<f32>(0.14);
    return clamp(numerator / denominator, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn cluster_index(pixel: vec2<f32>, depth: f32) -> u32 {
    let tiles_x = u32(frame.grid.x);
    let tiles_y = u32(frame.grid.y);
    let slices = u32(frame.grid.z);
    let tx = min(u32(pixel.x * frame.grid.x / frame.viewport.x), tiles_x - 1u);
    let ty = min(u32(pixel.y * frame.grid.y / frame.viewport.y), tiles_y - 1u);
    let near = frame.params.z;
    let far = frame.params.w;
    let d = max(depth, near);
    let t = log(d / near) / log(far / near);
    let s = min(u32(max(floor(t * f32(slices)), 0.0)), slices - 1u);
    return (s * tiles_y + ty) * tiles_x + tx;
}

fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (frame.shadow_params.z < 0.5) {
        return 1.0;
    }
    let texel = frame.shadow_params.y;
    let offset_world = world + n * frame.shadow_params.w;
    let clip = frame.shadow_view_proj * vec4<f32>(offset_world, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let depth = ndc.z - frame.shadow_params.x;
    let inside = uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0 && depth <= 1.0;
    let a = textureSampleCompare(shadow_map, shadow_sampler, uv + vec2<f32>(-0.5, -0.5) * texel, depth);
    let b = textureSampleCompare(shadow_map, shadow_sampler, uv + vec2<f32>(0.5, -0.5) * texel, depth);
    let c = textureSampleCompare(shadow_map, shadow_sampler, uv + vec2<f32>(-0.5, 0.5) * texel, depth);
    let d = textureSampleCompare(shadow_map, shadow_sampler, uv + vec2<f32>(0.5, 0.5) * texel, depth);
    let pcf = (a + b + c + d) * 0.25;
    return select(1.0, pcf, inside);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let rotation = mat3x3<f32>(frame.view[0].xyz, frame.view[1].xyz, frame.view[2].xyz);
    let eye = -(transpose(rotation) * frame.view[3].xyz);
    let surf = Surface(
        n,
        normalize(eye - input.world),
        input.world,
        input.color.rgb,
        input.material.x,
        input.material.y,
    );
    var direct = vec3<f32>(0.0);
    let directional_count = u32(frame.viewport.z);
    let visibility = shadow_factor(input.world, n);
    for (var i = 0u; i < directional_count; i++) {
        var contribution = shade(frame.directional[i], surf);
        if (i == 0u) {
            contribution = contribution * visibility;
        }
        direct = direct + contribution;
    }
    let cluster = cluster_index(input.clip.xy, input.depth);
    let range = cluster_ranges[cluster];
    for (var k = 0u; k < range.y; k++) {
        let light = lights[light_indices[range.x + k]];
        direct = direct + shade(light, surf);
    }
    let ambient = frame.params.y * input.color.rgb * (1.0 - input.material.x);
    let color = (direct + ambient) * frame.params.x;
    return vec4<f32>(aces(color), input.color.a);
}
