use std::ffi::CStr;
use std::path::{Path, PathBuf};

use crate::gl;
use crate::gl::types::*;
use crate::renderer::shader::{ShaderProgram, ShaderVersion};
use crate::renderer::Error;

const POST_PROCESS_SHADER_V: &str = include_str!("../../res/post_process.v.glsl");
const POST_PROCESS_SHADER_F: &str = include_str!("../../res/post_process.f.glsl");

/// Per-frame shader uniform values passed from the display to the post-process pass.
#[derive(Debug, Clone, Copy)]
pub struct ShaderUniformValues {
    pub resolution: [f32; 3],
    pub time: f32,
    pub time_delta: f32,
    pub frame: i32,
    pub frame_rate: f32,
    pub mouse: [f32; 4],
    pub current_cursor: [f32; 4],
    pub previous_cursor: [f32; 4],
    pub current_cursor_color: [f32; 4],
    pub previous_cursor_color: [f32; 4],
    pub current_cursor_style: i32,
    pub previous_cursor_style: i32,
    pub cursor_visible: i32,
    pub time_cursor_change: f32,
    pub focus: i32,
    pub time_focus: f32,
    pub background_color: [f32; 3],
    pub foreground_color: [f32; 3],
    pub cursor_color_rgb: [f32; 3],
    pub cursor_text: [f32; 3],
    pub selection_fg_color: [f32; 3],
    pub selection_bg_color: [f32; 3],
    pub sample_rate: f32,
    pub date: [f32; 4],
    pub channel_resolution: [[f32; 3]; 4],
    pub channel_time: [f32; 4],
}

#[derive(Debug)]
pub struct CustomShaderUniforms {
    pub i_channel0: GLint,
    pub i_resolution: GLint,
    pub i_time: GLint,
    pub i_time_delta: GLint,
    pub i_frame: GLint,
    pub i_mouse: GLint,
    pub i_date: GLint,
    pub i_current_cursor: GLint,
    pub i_previous_cursor: GLint,
    pub i_current_cursor_color: GLint,
    pub i_previous_cursor_color: GLint,
    pub i_current_cursor_style: GLint,
    pub i_previous_cursor_style: GLint,
    pub i_cursor_visible: GLint,
    pub i_time_cursor_change: GLint,
    pub i_time_focus: GLint,
    pub i_focus: GLint,
    pub i_background_color: GLint,
    pub i_foreground_color: GLint,
    pub i_cursor_color: GLint,
    pub i_cursor_text: GLint,
    pub i_selection_foreground_color: GLint,
    pub i_selection_background_color: GLint,
    pub i_frame_rate: GLint,
    pub i_sample_rate: GLint,
    pub i_channel_resolution: GLint,
    pub i_channel_time: GLint,
}

impl CustomShaderUniforms {
    pub fn from_program(program: &ShaderProgram) -> Self {
        Self {
            i_channel0: uniform_location(program, c"iChannel0"),
            i_resolution: uniform_location(program, c"iResolution"),
            i_time: uniform_location(program, c"iTime"),
            i_time_delta: uniform_location(program, c"iTimeDelta"),
            i_frame: uniform_location(program, c"iFrame"),
            i_mouse: uniform_location(program, c"iMouse"),
            i_date: uniform_location(program, c"iDate"),
            i_current_cursor: uniform_location(program, c"iCurrentCursor"),
            i_previous_cursor: uniform_location(program, c"iPreviousCursor"),
            i_current_cursor_color: uniform_location(program, c"iCurrentCursorColor"),
            i_previous_cursor_color: uniform_location(program, c"iPreviousCursorColor"),
            i_current_cursor_style: uniform_location(program, c"iCurrentCursorStyle"),
            i_previous_cursor_style: uniform_location(program, c"iPreviousCursorStyle"),
            i_cursor_visible: uniform_location(program, c"iCursorVisible"),
            i_time_cursor_change: uniform_location(program, c"iTimeCursorChange"),
            i_time_focus: uniform_location(program, c"iTimeFocus"),
            i_focus: uniform_location(program, c"iFocus"),
            i_background_color: uniform_location(program, c"iBackgroundColor"),
            i_foreground_color: uniform_location(program, c"iForegroundColor"),
            i_cursor_color: uniform_location(program, c"iCursorColor"),
            i_cursor_text: uniform_location(program, c"iCursorText"),
            i_selection_foreground_color: uniform_location(program, c"iSelectionForegroundColor"),
            i_selection_background_color: uniform_location(program, c"iSelectionBackgroundColor"),
            i_frame_rate: uniform_location(program, c"iFrameRate"),
            i_sample_rate: uniform_location(program, c"iSampleRate"),
            i_channel_resolution: uniform_location(program, c"iChannelResolution"),
            i_channel_time: uniform_location(program, c"iChannelTime"),
        }
    }

    pub fn apply(&self, v: &ShaderUniformValues) {
        Self::set_3f(self.i_resolution, &v.resolution);
        Self::set_1f(self.i_time, v.time);
        Self::set_1f(self.i_time_delta, v.time_delta);
        Self::set_1i(self.i_frame, v.frame);
        Self::set_1f(self.i_frame_rate, v.frame_rate);
        Self::set_4f(self.i_mouse, &v.mouse);
        Self::set_4f(self.i_current_cursor, &v.current_cursor);
        Self::set_4f(self.i_previous_cursor, &v.previous_cursor);
        Self::set_4f(self.i_current_cursor_color, &v.current_cursor_color);
        Self::set_4f(self.i_previous_cursor_color, &v.previous_cursor_color);
        Self::set_1i(self.i_current_cursor_style, v.current_cursor_style);
        Self::set_1i(self.i_previous_cursor_style, v.previous_cursor_style);
        Self::set_1i(self.i_cursor_visible, v.cursor_visible);
        Self::set_1f(self.i_time_cursor_change, v.time_cursor_change);
        Self::set_1f(self.i_time_focus, v.time_focus);
        Self::set_1i(self.i_focus, v.focus);
        Self::set_3f(self.i_background_color, &v.background_color);
        Self::set_3f(self.i_foreground_color, &v.foreground_color);
        Self::set_3f(self.i_cursor_color, &v.cursor_color_rgb);
        Self::set_3f(self.i_cursor_text, &v.cursor_text);
        Self::set_3f(self.i_selection_foreground_color, &v.selection_fg_color);
        Self::set_3f(self.i_selection_background_color, &v.selection_bg_color);
        Self::set_1f(self.i_sample_rate, v.sample_rate);
        Self::set_4f(self.i_date, &v.date);
        Self::set_3fv(self.i_channel_resolution, 4, &v.channel_resolution);
        Self::set_1fv(self.i_channel_time, 4, &v.channel_time);
    }

    fn set_1f(loc: GLint, v: f32) {
        if loc >= 0 {
            unsafe {
                gl::Uniform1f(loc, v);
            }
        }
    }

    fn set_1i(loc: GLint, v: i32) {
        if loc >= 0 {
            unsafe {
                gl::Uniform1i(loc, v);
            }
        }
    }

    fn set_3f(loc: GLint, v: &[f32; 3]) {
        if loc >= 0 {
            unsafe {
                gl::Uniform3f(loc, v[0], v[1], v[2]);
            }
        }
    }

    fn set_4f(loc: GLint, v: &[f32; 4]) {
        if loc >= 0 {
            unsafe {
                gl::Uniform4f(loc, v[0], v[1], v[2], v[3]);
            }
        }
    }

    fn set_3fv(loc: GLint, count: i32, data: &[[f32; 3]; 4]) {
        if loc >= 0 {
            let flat =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, 3 * data.len()) };
            unsafe {
                gl::Uniform3fv(loc, count, flat.as_ptr());
            }
        }
    }

    fn set_1fv(loc: GLint, count: i32, data: &[f32; 4]) {
        if loc >= 0 {
            unsafe {
                gl::Uniform1fv(loc, count, data.as_ptr());
            }
        }
    }
}

fn uniform_location(program: &ShaderProgram, name: &CStr) -> GLint {
    unsafe { gl::GetUniformLocation(program.id(), name.as_ptr()) }
}

#[derive(Debug)]
pub struct PostProcessState {
    fbo: GLuint,
    color_texture: GLuint,
    width: u32,
    height: u32,
    vao: GLuint,
    vbo: GLuint,
    passthrough_program: ShaderProgram,
    u_screen_texture: GLint,
    ping_texture: GLuint,
    ping_fbo: GLuint,
    custom_shaders: Vec<(ShaderProgram, CustomShaderUniforms)>,
}

impl PostProcessState {
    pub fn new() -> Result<Self, Error> {
        let passthrough_program = ShaderProgram::new(
            ShaderVersion::Glsl3,
            None,
            POST_PROCESS_SHADER_V,
            POST_PROCESS_SHADER_F,
        )?;
        let u_screen_texture = passthrough_program.get_uniform_location(c"screenTexture")?;

        let mut fbo: GLuint = 0;
        let mut color_texture: GLuint = 0;
        let mut ping_texture: GLuint = 0;
        let mut ping_fbo: GLuint = 0;
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut fbo);
            gl::GenTextures(1, &mut color_texture);
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            // Dummy vertices for macOS: the driver requires at least one enabled
            // vertex attribute in the VAO even when the shader uses gl_VertexID only.
            let dummy_vertices: [f32; 6] = [0.0; 6];
            gl::BufferData(
                gl::ARRAY_BUFFER,
                std::mem::size_of_val(&dummy_vertices) as isize,
                dummy_vertices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE,
                (2 * std::mem::size_of::<f32>()) as i32,
                std::ptr::null(),
            );
            gl::EnableVertexAttribArray(0);

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            // Use GL_TEXTURE1 to avoid disturbing the text renderer's glyph
            // atlas on GL_TEXTURE0.  The text renderer caches which texture
            // is bound to unit 0 in `active_tex` and skips re-binding when
            // the ID matches; touching unit 0 here would silently break text.
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, color_texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            // Initialize with actual zero data to avoid macOS "texture unloadable" warnings.
            let zero_pixel: [u8; 4] = [0, 0, 0, 255];
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                1,
                1,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                zero_pixel.as_ptr() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                color_texture,
                0,
            );

            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

            if status != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &fbo);
                gl::DeleteTextures(1, &color_texture);
                gl::DeleteBuffers(1, &vbo);
                gl::DeleteVertexArrays(1, &vao);
                return Err(format!("Framebuffer is not complete: {status}").into());
            }

            gl::GenTextures(1, &mut ping_texture);
            gl::GenFramebuffers(1, &mut ping_fbo);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, ping_texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                1,
                1,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                zero_pixel.as_ptr() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            gl::BindFramebuffer(gl::FRAMEBUFFER, ping_fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                ping_texture,
                0,
            );

            let ping_status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

            if ping_status != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &ping_fbo);
                gl::DeleteTextures(1, &ping_texture);
                gl::DeleteFramebuffers(1, &fbo);
                gl::DeleteTextures(1, &color_texture);
                gl::DeleteBuffers(1, &vbo);
                gl::DeleteVertexArrays(1, &vao);
                return Err(format!("Ping framebuffer is not complete: {ping_status}").into());
            }
        }

        Ok(Self {
            fbo,
            color_texture,
            width: 1,
            height: 1,
            vao,
            vbo,
            passthrough_program,
            u_screen_texture,
            ping_texture,
            ping_fbo,
            custom_shaders: Vec::new(),
        })
    }

    pub fn has_custom_shader(&self) -> bool {
        !self.custom_shaders.is_empty()
    }

    /// Load multiple custom fragment shaders from the filesystem.
    pub fn load_custom_shaders(&mut self, paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
        let mut errors = Vec::new();
        let mut shaders = Vec::new();

        let prefix = include_str!("../../res/shadertoy_prefix.glsl");
        let vertex = include_str!("../../res/post_process.v.glsl");

        for path in paths {
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push((path.clone(), format!("read error: {e}")));
                    continue;
                },
            };
            let processed = preprocess_shader_for_macos(&source);
            match ShaderProgram::new_post_process(vertex, prefix, &processed) {
                Ok(program) => {
                    let uniforms = CustomShaderUniforms::from_program(&program);
                    shaders.push((program, uniforms));
                },
                Err(e) => {
                    errors.push((path.clone(), format!("compile error: {e}")));
                },
            }
        }

        self.custom_shaders = shaders;
        errors
    }

    /// Load a single custom fragment shader (backward-compatible wrapper).
    #[allow(dead_code)]
    pub fn load_custom_shader(&mut self, path: &Path) -> Result<(), String> {
        let errors = self.load_custom_shaders(&[path.to_path_buf()]);
        if let Some((_, e)) = errors.into_iter().next() {
            Err(e)
        } else if self.custom_shaders.is_empty() {
            Err("No shader loaded".to_owned())
        } else {
            Ok(())
        }
    }

    /// Remove loaded custom shaders, reverting to the passthrough post-process pass.
    pub fn clear_custom_shader(&mut self) {
        self.custom_shaders.clear();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        unsafe {
            // Use GL_TEXTURE1 to avoid disturbing the text renderer's glyph
            // atlas on GL_TEXTURE0.  The text renderer caches which texture
            // is bound to unit 0 in `active_tex` and skips re-binding when
            // the ID matches; touching unit 0 here would silently break text.
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.color_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D, 0, gl::RGBA8 as i32,
                width as i32, height as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE, std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            // Re-attach the color texture to the FBO.
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D, self.color_texture, 0,
            );
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.ping_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D, 0, gl::RGBA8 as i32,
                width as i32, height as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE, std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.ping_fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D, self.ping_texture, 0,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }

    pub fn bind_fbo(&self) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
        }
    }

    pub fn draw(&self, width: u32, height: u32, uniforms: &ShaderUniformValues) {
        unsafe {
            gl::Disable(gl::BLEND);

            let shader_count = self.custom_shaders.len();
            if shader_count <= 1 {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::Viewport(0, 0, width as i32, height as i32);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                let channel0_loc = if let Some((prog, u)) = self.custom_shaders.first() {
                    gl::UseProgram(prog.id());
                    u.apply(uniforms);
                    u.i_channel0
                } else {
                    gl::UseProgram(self.passthrough_program.id());
                    self.u_screen_texture
                };

                gl::ActiveTexture(gl::TEXTURE1);
                gl::BindTexture(gl::TEXTURE_2D, self.color_texture);
                if channel0_loc >= 0 {
                    gl::Uniform1i(channel0_loc, 1);
                }

                gl::BindVertexArray(self.vao);
                gl::DrawArrays(gl::TRIANGLES, 0, 3);
                gl::BindVertexArray(0);
            } else {
                let mut back = self.color_texture;
                let mut front = self.ping_texture;

                for (i, (prog, u)) in self.custom_shaders.iter().enumerate() {
                    let is_last = i == shader_count - 1;

                    if is_last {
                        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                        gl::Viewport(0, 0, width as i32, height as i32);
                        gl::Clear(gl::COLOR_BUFFER_BIT);
                    } else {
                        gl::BindFramebuffer(gl::FRAMEBUFFER, self.ping_fbo);
                        gl::FramebufferTexture2D(
                            gl::FRAMEBUFFER,
                            gl::COLOR_ATTACHMENT0,
                            gl::TEXTURE_2D,
                            front,
                            0,
                        );
                        gl::Viewport(0, 0, width as i32, height as i32);
                    }

                    gl::UseProgram(prog.id());
                    u.apply(uniforms);

                    gl::ActiveTexture(gl::TEXTURE1);
                    gl::BindTexture(gl::TEXTURE_2D, back);
                    if u.i_channel0 >= 0 {
                        gl::Uniform1i(u.i_channel0, 1);
                    }

                    gl::BindVertexArray(self.vao);
                    gl::DrawArrays(gl::TRIANGLES, 0, 3);
                    gl::BindVertexArray(0);

                    if !is_last {
                        std::mem::swap(&mut back, &mut front);
                    }
                }
            }

            gl::UseProgram(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC1_COLOR, gl::ONE_MINUS_SRC1_COLOR);
        }
    }
}

impl Drop for PostProcessState {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteFramebuffers(1, &self.ping_fbo);
            gl::DeleteTextures(1, &self.color_texture);
            gl::DeleteTextures(1, &self.ping_texture);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

/// GLSL type keywords that begin a variable declaration at global scope.
const GLSL_TYPE_PREFIXES: &[&str] = &[
    "vec2 ", "vec3 ", "vec4 ", "float ", "int ", "uint ", "bool ",
    "mat2 ", "mat3 ", "mat4 ", "ivec2 ", "ivec3 ", "ivec4 ",
    "uvec2 ", "uvec3 ", "uvec4 ", "bvec2 ", "bvec3 ", "bvec4 ",
    "mat2x2 ", "mat2x3 ", "mat2x4 ", "mat3x2 ", "mat3x3 ", "mat3x4 ",
    "mat4x2 ", "mat4x3 ", "mat4x4 ",
];

/// GLSL built-in constructor names that look like function calls but are not.
const GLSL_CONSTRUCTORS: &[&str] = &[
    "vec2", "vec3", "vec4", "float", "int", "uint", "bool",
    "mat2", "mat3", "mat4", "ivec2", "ivec3", "ivec4",
    "uvec2", "uvec3", "uvec4", "bvec2", "bvec3", "bvec4",
    "min", "max", "clamp", "mix", "step", "smoothstep", "pow",
    "abs", "sign", "floor", "ceil", "fract", "mod", "sqrt",
    "sin", "cos", "tan", "asin", "acos", "atan", "exp", "log",
    "exp2", "log2", "length", "distance", "dot", "cross",
];

/// Check whether a GLSL expression contains a call to a user-defined function.
///
/// This detects `identifier(` patterns where the identifier is NOT a GLSL
/// built-in type constructor. Used to identify global variable initializers
/// that crash macOS's GLSL linker (the `glpLLVMGetFunctionGlobalVariableUse`
/// null-pointer bug).
fn has_user_function_call(expr: &str) -> bool {
    let bytes = expr.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'(' && i > 0 {
            let end = i;
            let mut start = i;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_')
            {
                start -= 1;
            }
            if start < end {
                let ident = &expr[start..end];
                if !ident.is_empty() && !GLSL_CONSTRUCTORS.contains(&ident) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Preprocess shader source to work around macOS OpenGL driver bugs.
///
/// macOS's GLSL linker crashes in `glpLLVMGetFunctionGlobalVariableUse`
/// when a non-const global variable is initialized with an expression that
/// contains a user-defined function call (e.g.
/// `vec4 COLOR = vec4(myFunc(uniform_var.rgb), 1.0);`). This is a known
/// Apple driver bug affecting cursor-shaders like `cursor_sweep.glsl`.
///
/// This function detects such declarations, strips the initializer from the
/// global scope, and defers it to a `_alacritty_init_globals()` function
/// that runs per fragment invocation, immediately before `mainImage()`.
fn preprocess_shader_for_macos(source: &str) -> String {
    let mut output = String::with_capacity(source.len() + 256);
    let mut deferred_inits: Vec<String> = Vec::new();
    let mut brace_depth: i32 = 0;

    for line in source.lines() {
        let trimmed = line.trim();
        let depth_at_start = brace_depth;

        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                _ => {},
            }
        }

        if depth_at_start == 0 && !trimmed.is_empty() && should_defer_global(trimmed) {
            if let Some((decl_only, var_name, init_expr)) = split_global_declaration(trimmed) {
                output.push_str(&decl_only);
                output.push('\n');
                deferred_inits.push(format!("    {var_name} = {init_expr};"));
                continue;
            }
        }

        output.push_str(line);
        output.push('\n');
    }

    output.push_str("\nvoid _alacritty_init_globals() {\n");
    for init in &deferred_inits {
        output.push_str(init);
        output.push('\n');
    }
    output.push_str("}\n");

    output
}

/// Return true if a global-scope line is a non-const variable declaration
/// whose initializer contains a user-defined function call.
fn should_defer_global(line: &str) -> bool {
    if line.starts_with("const ")
        || line.starts_with("uniform ")
        || line.starts_with("in ")
        || line.starts_with("out ")
        || line.starts_with("layout")
        || line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with("void ")
        || line.starts_with("struct ")
    {
        return false;
    }

    let is_type_decl = GLSL_TYPE_PREFIXES.iter().any(|t| line.starts_with(t));
    if !is_type_decl {
        return false;
    }

    if let Some(eq_pos) = line.find(" = ") {
        let after_eq = &line[eq_pos + 3..];
        return has_user_function_call(after_eq);
    }

    false
}

/// Split `vec4 TRAIL_COLOR = vec4(func(...));` into
/// `("vec4 TRAIL_COLOR;", "TRAIL_COLOR", "vec4(func(...))")`.
fn split_global_declaration(line: &str) -> Option<(String, String, String)> {
    let eq_pos = line.find(" = ")?;
    let before_eq = line[..eq_pos].trim();
    let after_eq = line[eq_pos + 3..].trim().trim_end_matches(';');

    let var_name = before_eq.split_whitespace().last()?;
    let decl_only = format!("{before_eq};");

    Some((decl_only, var_name.to_string(), after_eq.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_user_function_call_detects_user_func() {
        assert!(has_user_function_call("vec4(sRGBToLinear(x.rgb), x.a)"));
        assert!(has_user_function_call("myFunc(1.0)"));
        assert!(has_user_function_call("vec4(customCalc(u.x), 0.0, 0.0, 1.0)"));
    }

    #[test]
    fn test_has_user_function_call_ignores_builtins() {
        assert!(!has_user_function_call("vec4(0.35, 0.36, 0.44, 1.0)"));
        assert!(!has_user_function_call("vec4(mix(vec3(0.0), vec3(1.0), 0.5), 1.0)"));
        assert!(!has_user_function_call("vec3(1.0, 2.0, 3.0)"));
        assert!(!has_user_function_call("float(42)"));
        assert!(!has_user_function_call("pow(2.0, 3.0)"));
    }

    #[test]
    fn test_should_defer_global_trail_color() {
        let line = "vec4 TRAIL_COLOR = vec4(sRGBToLinear(iCurrentCursorColor.rgb), iCurrentCursorColor.a);";
        assert!(should_defer_global(line));
    }

    #[test]
    fn test_should_not_defer_const_literal() {
        let line = "vec4 COLOR = vec4(0.35, 0.36, 0.44, 1.0);";
        assert!(!should_defer_global(line));
    }

    #[test]
    fn test_should_not_defer_const_decl() {
        let line = "const float DURATION = 0.2;";
        assert!(!should_defer_global(line));
    }

    #[test]
    fn test_preprocess_defers_trail_color() {
        let source = r#"vec3 sRGBToLinear(vec3 c) {
    return c;
}

vec4 TRAIL_COLOR = vec4(sRGBToLinear(iCurrentCursorColor.rgb), iCurrentCursorColor.a);
const float DURATION = 0.2;

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = TRAIL_COLOR;
}
"#;
        let result = preprocess_shader_for_macos(source);
        assert!(result.contains("vec4 TRAIL_COLOR;"));
        assert!(!result.contains("vec4 TRAIL_COLOR = vec4(sRGBToLinear"));
        assert!(result.contains("_alacritty_init_globals"));
        assert!(result.contains("TRAIL_COLOR = vec4(sRGBToLinear(iCurrentCursorColor.rgb), iCurrentCursorColor.a)"));
        assert!(result.contains("const float DURATION = 0.2;"));
    }

    #[test]
    fn test_preprocess_noop_safe_shader() {
        let source = r#"vec4 COLOR = vec4(0.35, 0.36, 0.44, 1.0);
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = COLOR;
}
"#;
        let result = preprocess_shader_for_macos(source);
        assert!(result.contains("vec4 COLOR = vec4(0.35, 0.36, 0.44, 1.0);"));
        assert!(result.contains("void _alacritty_init_globals() {\n}"));
    }
}
