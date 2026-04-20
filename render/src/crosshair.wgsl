@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    // 12 vertices for a '+' shape crosshair
    // Vertical rect: (-0.002, -0.03) to (0.002, 0.03)
    // Horizontal rect: (-0.02, -0.003) to (0.02, 0.003)
    var pos = array<vec2<f32>, 12>(
        // Vertical triangle 1
        vec2<f32>(-0.002,  0.03),
        vec2<f32>( 0.002,  0.03),
        vec2<f32>(-0.002, -0.03),
        // Vertical triangle 2
        vec2<f32>( 0.002,  0.03),
        vec2<f32>( 0.002, -0.03),
        vec2<f32>(-0.002, -0.03),
        
        // Horizontal triangle 1
        vec2<f32>(-0.02,  0.003),
        vec2<f32>( 0.02,  0.003),
        vec2<f32>(-0.02, -0.003),
        // Horizontal triangle 2
        vec2<f32>( 0.02,  0.003),
        vec2<f32>( 0.02, -0.003),
        vec2<f32>(-0.02, -0.003)
    );
    return vec4<f32>(pos[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.8);
}
