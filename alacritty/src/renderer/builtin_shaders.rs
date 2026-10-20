/// Return the GLSL source for a built-in shader by name, or `None`.
pub fn get_builtin_shader(name: &str) -> Option<&'static str> {
    match name {
        "cursor_sweep" => Some(include_str!("../../res/builtin_shaders/cursor_sweep.glsl")),
        "cursor_tail" => Some(include_str!("../../res/builtin_shaders/cursor_tail.glsl")),
        "cursor_warp" => Some(include_str!("../../res/builtin_shaders/cursor_warp.glsl")),
        "rectangle_boom" => {
            Some(include_str!("../../res/builtin_shaders/rectangle_boom_cursor.glsl"))
        },
        "ripple" => Some(include_str!("../../res/builtin_shaders/ripple_cursor.glsl")),
        "ripple_rectangle" => {
            Some(include_str!("../../res/builtin_shaders/ripple_rectangle_cursor.glsl"))
        },
        "sonic_boom" => Some(include_str!("../../res/builtin_shaders/sonic_boom_cursor.glsl")),
        _ => None,
    }
}
