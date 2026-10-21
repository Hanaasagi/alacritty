use std::ffi::CStr;
use std::path::{Path, PathBuf};

use crate::gl;
use crate::gl::types::*;
use crate::renderer::Error;
use crate::renderer::shader::ShaderProgram;

const POST_PROCESS_SHADER_V: &str = include_str!("../../res/post_process.v.glsl");

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
    pub date: [f32; 4],
    pub channel_resolution: [[f32; 3]; 4],
    pub palette: [[f32; 3]; 256],
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
    pub i_channel_resolution: GLint,
    pub i_palette: GLint,
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
            i_channel_resolution: uniform_location(program, c"iChannelResolution"),
            i_palette: uniform_location(program, c"iPalette"),
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
        Self::set_4f(self.i_date, &v.date);
        Self::set_3fv(self.i_channel_resolution, &v.channel_resolution);
        Self::set_3fv(self.i_palette, &v.palette);
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

    fn set_3fv(loc: GLint, data: &[[f32; 3]]) {
        if loc >= 0 {
            let flat =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, 3 * data.len()) };
            unsafe {
                gl::Uniform3fv(loc, data.len() as i32, flat.as_ptr());
            }
        }
    }
}

fn uniform_location(program: &ShaderProgram, name: &CStr) -> GLint {
    unsafe { gl::GetUniformLocation(program.id(), name.as_ptr()) }
}

/// A shader to load from disk or from an in-memory GLSL source string.
#[derive(Debug, Clone)]
pub enum ShaderEntry {
    File(PathBuf),
    Source { label: PathBuf, source: &'static str },
}

impl ShaderEntry {
    /// Label used to identify the shader in error messages.
    pub fn label(&self) -> &Path {
        match self {
            Self::File(path) => path,
            Self::Source { label, .. } => label,
        }
    }
}

#[derive(Debug)]
pub struct PostProcessState {
    fbo: GLuint,
    color_texture: GLuint,
    width: u32,
    height: u32,
    vao: GLuint,
    vbo: GLuint,
    ping_texture: GLuint,
    ping_fbo: GLuint,
    custom_shaders: Vec<(ShaderProgram, CustomShaderUniforms)>,
}

impl PostProcessState {
    pub fn new() -> Result<Self, Error> {
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
            ping_texture,
            ping_fbo,
            custom_shaders: Vec::new(),
        })
    }

    pub fn has_custom_shader(&self) -> bool {
        !self.custom_shaders.is_empty()
    }

    /// Load shaders from a mix of file paths and in-memory GLSL source.
    pub fn load_shaders(&mut self, entries: &[ShaderEntry]) -> Vec<(PathBuf, String)> {
        let mut errors = Vec::new();
        let mut shaders = Vec::new();

        let prefix = include_str!("../../res/shadertoy_prefix.glsl");

        for entry in entries {
            let (label, source_result) = match entry {
                ShaderEntry::File(path) => {
                    let source =
                        std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"));
                    (path.clone(), source)
                },
                ShaderEntry::Source { label, source } => (label.clone(), Ok((*source).to_owned())),
            };

            let source = match source_result {
                Ok(s) => s,
                Err(e) => {
                    errors.push((label, e));
                    continue;
                },
            };

            // The macOS OpenGL linker segfaults on non-const global initializers
            // calling user-defined functions; reject such shaders with an
            // actionable error instead of crashing the process.
            if cfg!(target_os = "macos") {
                if let Some(decl) = detect_macos_unsafe_global(&source) {
                    errors.push((
                        label,
                        format!(
                            "global initializer calls a user-defined function, which crashes \
                             the macOS OpenGL linker: `{decl}`. Assign it inside mainImage() \
                             instead"
                        ),
                    ));
                    continue;
                }
            }

            match ShaderProgram::new_post_process(POST_PROCESS_SHADER_V, prefix, &source) {
                Ok(program) => {
                    let uniforms = CustomShaderUniforms::from_program(&program);
                    shaders.push((program, uniforms));
                },
                Err(e) => {
                    errors.push((label, format!("compile error: {e}")));
                },
            }
        }

        self.custom_shaders = shaders;
        errors
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
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            // Re-attach the color texture to the FBO.
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.color_texture,
                0,
            );
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.ping_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_BASE_LEVEL, 0);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.ping_fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.ping_texture,
                0,
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
        let shader_count = self.custom_shaders.len();
        if shader_count == 0 {
            return;
        }

        unsafe {
            gl::Disable(gl::BLEND);

            if shader_count == 1 {
                let (prog, u) = &self.custom_shaders[0];
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::Viewport(0, 0, width as i32, height as i32);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                gl::UseProgram(prog.id());
                u.apply(uniforms);

                gl::ActiveTexture(gl::TEXTURE1);
                gl::BindTexture(gl::TEXTURE_2D, self.color_texture);
                if u.i_channel0 >= 0 {
                    gl::Uniform1i(u.i_channel0, 1);
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
    "vec2", "vec3", "vec4", "float", "int", "uint", "bool", "mat2", "mat3", "mat4", "ivec2",
    "ivec3", "ivec4", "uvec2", "uvec3", "uvec4", "bvec2", "bvec3", "bvec4",
];

/// Detect a global-scope variable declaration whose initializer calls a user-defined
/// function, e.g. `vec4 C = vec4(myFunc(x), 1.0);`.
///
/// macOS's OpenGL linker crashes in `glpLLVMGetFunctionGlobalVariableUse` on such
/// declarations. Returns the offending declaration for error reporting.
///
/// Heuristic, intentionally conservative: strips comments and string literals so they
/// can't pollute scope tracking, collects function names from `name(...)` signatures and
/// prototypes at global scope, then flags non-const global initializers referencing them.
/// False positives reject a shader with a clear error; false negatives crash the linker,
/// so the checks err on the side of flagging.
fn detect_macos_unsafe_global(source: &str) -> Option<String> {
    let mut fn_names: Vec<String> = Vec::new();
    let mut decls: Vec<String> = Vec::new();

    // Split the source into global-scope chunks: function definitions end with `{`,
    // declarations and prototypes end with `;`.
    let mut chunk = String::new();
    let mut depth: i32 = 0;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Strip line comments.
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            },
            // Strip block comments.
            '/' if chars.peek() == Some(&'*') => {
                let mut prev = ' ';
                for c in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                continue;
            },
            // Strip string literals.
            '"' => {
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                }
                continue;
            },
            // Preprocessor directives glue onto the next statement otherwise.
            '#' if depth == 0 && chunk.trim().is_empty() => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            },
            _ => {},
        }

        if depth == 0 {
            match c {
                '{' => {
                    depth += 1;
                    // `{` at global scope ends a function signature; any return type is
                    // accepted so user-defined struct returns are covered.
                    if let Some(name) = signature_fn_name(&chunk, false) {
                        fn_names.push(name);
                    }
                    chunk.clear();
                },
                ';' => {
                    // `;` at global scope ends a declaration or a function prototype.
                    if let Some(name) = signature_fn_name(&chunk, true) {
                        fn_names.push(name);
                    } else {
                        decls.push(chunk.trim().to_string());
                    }
                    chunk.clear();
                },
                _ => chunk.push(c),
            }
        } else {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {},
            }
        }
    }

    for decl in &decls {
        if !GLSL_TYPE_PREFIXES.iter().any(|t| decl.starts_with(t)) {
            continue;
        }
        let Some(eq) = find_assignment_eq(decl) else { continue };
        let init = &decl[eq + 1..];
        if fn_names.iter().any(|name| contains_call(init, name)) {
            return Some(decl.lines().next().unwrap_or(decl).trim().to_string());
        }
    }
    None
}

/// Extract the function name from a `type name(params)` chunk. With `require_type`,
/// the token before the name must be a GLSL type keyword (for prototypes, where the
/// chunk shape is ambiguous with variable declarations).
fn signature_fn_name(chunk: &str, require_type: bool) -> Option<String> {
    let paren = chunk.find('(')?;
    let before = chunk[..paren].trim_end();
    // An assignment before the first paren means this is a variable declaration.
    if before.contains('=') {
        return None;
    }

    let name_start =
        before.rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let name = &before[name_start..];
    if name.is_empty() {
        return None;
    }

    if require_type {
        let ret = before[..name_start].trim();
        let is_type = ret == "void" || GLSL_TYPE_PREFIXES.contains(&ret);
        if !is_type {
            return None;
        }
    }

    Some(name.to_string())
}

/// Find the first assignment `=` (not `==`, `<=`, `>=`, `!=`).
fn find_assignment_eq(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'=' {
            let next_is_eq = bytes.get(i + 1) == Some(&b'=');
            let prev = i.checked_sub(1).map(|j| bytes[j]);
            let prev_op = matches!(prev, Some(b'<') | Some(b'>') | Some(b'=') | Some(b'!'));
            if !next_is_eq && !prev_op {
                return Some(i);
            }
        }
    }
    None
}

/// Whether `expr` contains a call to `name` (whole identifier followed by `(`).
fn contains_call(expr: &str, name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = expr[start..].find(name) {
        let abs = start + pos;
        let before_ok = abs == 0 || {
            let c = expr.as_bytes()[abs - 1];
            !(c.is_ascii_alphanumeric() || c == b'_')
        };
        if before_ok && expr[abs + name.len()..].trim_start().starts_with('(') {
            return true;
        }
        start = abs + name.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_classic_pattern() {
        let source = r#"vec3 sRGBToLinear(vec3 c) {
    return c;
}

vec4 TRAIL_COLOR = vec4(sRGBToLinear(iCurrentCursorColor.rgb), iCurrentCursorColor.a);

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = TRAIL_COLOR;
}
"#;
        assert_eq!(
            detect_macos_unsafe_global(source).as_deref(),
            Some("vec4 TRAIL_COLOR = vec4(sRGBToLinear(iCurrentCursorColor.rgb), iCurrentCursorColor.a)")
        );
    }

    #[test]
    fn detects_without_spaces_around_eq() {
        let source = r#"float helper(float x) { return x; }
float value=helper(1.0);
"#;
        assert!(detect_macos_unsafe_global(source).is_some());
    }

    #[test]
    fn detects_multiline_initializer() {
        let source = r#"float helper(float x) { return x; }
float value =
    helper(1.0);
"#;
        assert!(detect_macos_unsafe_global(source).is_some());
    }

    #[test]
    fn detects_via_prototype() {
        let source = r#"float helper(float x);
float value = helper(1.0);
float helper(float x) { return x; }
"#;
        assert!(detect_macos_unsafe_global(source).is_some());
    }

    #[test]
    fn ignores_const_literal() {
        let source = r#"vec4 COLOR = vec4(0.35, 0.36, 0.44, 1.0);
const float DURATION = 0.2;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = COLOR;
}
"#;
        assert_eq!(detect_macos_unsafe_global(source), None);
    }

    #[test]
    fn ignores_builtin_calls() {
        let source = r#"float X = pow(2.0, 3.0);
vec3 Y = normalize(vec3(X));
"#;
        assert_eq!(detect_macos_unsafe_global(source), None);
    }

    #[test]
    fn comment_braces_do_not_pollute_scope() {
        let source = r#"// { not a scope
float helper(float x) { return x; } // }
float value = helper(1.0);
"#;
        assert!(detect_macos_unsafe_global(source).is_some());
    }

    #[test]
    fn usage_inside_function_body_is_fine() {
        let source = r#"float helper(float x) { return x; }
float value;
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    value = helper(1.0);
    fragColor = vec4(value);
}
"#;
        assert_eq!(detect_macos_unsafe_global(source), None);
    }
}
