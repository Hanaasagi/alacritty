use crate::gl;
use crate::gl::types::*;
use crate::renderer::shader::{ShaderProgram, ShaderVersion};
use crate::renderer::Error;

const POST_PROCESS_SHADER_V: &str = include_str!("../../res/post_process.v.glsl");
const POST_PROCESS_SHADER_F: &str = include_str!("../../res/post_process.f.glsl");

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
        })
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
            // Resize the FBO render target.
            gl::BindTexture(gl::TEXTURE_2D, self.color_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D, 0, gl::RGBA8 as i32,
                width as i32, height as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE, std::ptr::null(),
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);

            // Re-attach the color texture to the FBO.
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D, self.color_texture, 0,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }

    pub fn bind_fbo(&self) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
        }
    }

    pub fn draw(&self, width: u32, height: u32) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, width as i32, height as i32);

            gl::Disable(gl::BLEND);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(self.passthrough_program.id());

            // Use GL_TEXTURE1 for the FBO texture to avoid disturbing
            // GL_TEXTURE0 which the text renderer's glyph atlas occupies.
            // The text renderer tracks its binding state and skips re-binding
            // if it thinks the atlas is already on unit 0.
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, self.color_texture);
            gl::Uniform1i(self.u_screen_texture, 1);

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);

            gl::BindVertexArray(0);
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
            gl::DeleteTextures(1, &self.color_texture);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}
