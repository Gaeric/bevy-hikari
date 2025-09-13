#define_import_path bevy_hikari::overlay

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_core_pipeline::tonemapping::tonemapping_luminance as luminance

@group(0) @binding(0)
var render_texture_2d: texture_2d<f32>;
@group(0) @binding(1)
var render_sampler: sampler;

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

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(render_texture_2d, render_sampler, in.uv);
    return tone_mapping(color);
}
