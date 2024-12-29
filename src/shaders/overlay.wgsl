#define_import_path bevy_hikari::overlay

@group(0) @binding(0)
var render_texture_2d: texture_2d<f32>;
@group(0) @binding(1)
var render_sampler: sampler;

// luminance coefficients from Rec. 709.
// https://en.wikipedia.org/wiki/Rec._709
fn luminance(v: vec3<f32>) -> f32 {
    return dot(v, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn change_luminance(c_in: vec3<f32>, l_out: f32) -> vec3<f32> {
    let l_in = luminance(c_in);
    return c_in * (l_out / l_in);
}

fn reinhard_luminance(color: vec3<f32>) -> vec3<f32> {
    let l_old = luminance(color);
    let l_new = l_old / (1.0 + l_old);
    return change_luminance(color, l_new);
}

fn tone_mapping(in: vec4<f32>) -> vec4<f32> {
    // tone_mapping
    return vec4<f32>(reinhard_luminance(in.rgb), in.a);
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
};

@vertex
fn vertex(@location(0) position: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 1.0);
    out.position = position;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = 0.5 * in.position.xy + 0.5;
    uv.y = 1.0 - uv.y;
    let color = textureSample(render_texture_2d, render_sampler, uv);
    return tone_mapping(color);
}
