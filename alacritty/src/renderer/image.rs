use std::collections::HashMap;
use std::mem;
use std::ptr;

use log::warn;

use crate::display::SizeInfo;
use crate::gl::types::*;
use crate::renderer::shader::{ShaderProgram, ShaderVersion};
use crate::{gl, renderer};

const IMAGE_SHADER_V_GLSL3: &str = include_str!("../../res/glsl3/image.v.glsl");
const IMAGE_SHADER_F_GLSL3: &str = include_str!("../../res/glsl3/image.f.glsl");
const IMAGE_SHADER_V_GLES2: &str = include_str!("../../res/gles2/image.v.glsl");
const IMAGE_SHADER_F_GLES2: &str = include_str!("../../res/gles2/image.f.glsl");

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    tex: [f32; 2],
}

#[derive(Debug)]
struct GpuTexture {
    id: GLuint,
    width: u32,
    height: u32,
}

#[derive(Debug)]
pub struct ImageRenderer {
    program: ShaderProgram,
    vao: GLuint,
    vbo: GLuint,
    u_image_tex: GLint,
    textures: HashMap<u32, GpuTexture>,
}

impl ImageRenderer {
    pub fn new(shader_version: ShaderVersion) -> Result<Self, renderer::Error> {
        let (vertex, fragment) = match shader_version {
            ShaderVersion::Glsl3 => (IMAGE_SHADER_V_GLSL3, IMAGE_SHADER_F_GLSL3),
            ShaderVersion::Gles2 => (IMAGE_SHADER_V_GLES2, IMAGE_SHADER_F_GLES2),
        };

        let program = ShaderProgram::new(shader_version, None, vertex, fragment)?;
        let u_image_tex = program.get_uniform_location(c"imageTex")?;

        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            let stride = mem::size_of::<Vertex>() as i32;
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, stride, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE,
                stride,
                (2 * mem::size_of::<f32>()) as *const _,
            );
            gl::EnableVertexAttribArray(1);

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(Self { program, vao, vbo, u_image_tex, textures: HashMap::new() })
    }

    pub fn upload_image(&mut self, image_id: u32, width: u32, height: u32, data: &[u8]) {
        let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        if width == 0 || height == 0 || data.len() < expected {
            warn!("Skipping image upload for id {image_id}: invalid dimensions or data");
            return;
        }

        unsafe {
            // Run all image texture ops on unit 1: the text renderer caches
            // which texture is bound to GL_TEXTURE0 and skips re-binding, so
            // touching unit 0 would silently clobber the glyph atlas (same
            // convention as the post-process pass).
            gl::ActiveTexture(gl::TEXTURE1);
            if let Some(existing) = self.textures.get(&image_id) {
                if existing.width == width && existing.height == height {
                    gl::BindTexture(gl::TEXTURE_2D, existing.id);
                    gl::TexSubImage2D(
                        gl::TEXTURE_2D,
                        0,
                        0,
                        0,
                        width as i32,
                        height as i32,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        data.as_ptr() as *const _,
                    );
                    gl::BindTexture(gl::TEXTURE_2D, 0);
                    gl::ActiveTexture(gl::TEXTURE0);
                    return;
                }
                self.remove_image(image_id);
            }

            let mut texture = 0;
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                width as i32,
                height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                data.as_ptr() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);

            self.textures.insert(image_id, GpuTexture { id: texture, width, height });
        }
    }

    pub fn draw_image(
        &mut self,
        image_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        source_rect: Option<[f32; 4]>,
        size_info: &SizeInfo,
    ) {
        let Some(texture) = self.textures.get(&image_id) else {
            return;
        };

        let img_w = texture.width as f32;
        let img_h = texture.height as f32;
        let (src_x, src_y, src_w, src_h) = match source_rect {
            Some([sx, sy, sw, sh]) if sw > 0.0 && sh > 0.0 => {
                let sx = sx.clamp(0.0, img_w);
                let sy = sy.clamp(0.0, img_h);
                let sw = sw.min(img_w - sx);
                let sh = sh.min(img_h - sy);
                (sx, sy, sw.max(0.0), sh.max(0.0))
            },
            _ => (0.0, 0.0, img_w, img_h),
        };

        let u0 = src_x / img_w;
        let u1 = (src_x + src_w) / img_w;
        // glTexImage2D stores the first data row at the texture bottom (v=0),
        // which corresponds to the image top.  Map the quad top to v_top (low v)
        // and the quad bottom to v_bottom (high v) so the image is right-side up.
        let v_top = src_y / img_h;
        let v_bottom = (src_y + src_h) / img_h;

        let (ndc_x, ndc_y, ndc_w, ndc_h) = pixels_to_ndc(x, y, width, height, size_info);

        let vertices = [
            Vertex { pos: [ndc_x, ndc_y], tex: [u0, v_top] },
            Vertex { pos: [ndc_x, ndc_y - ndc_h], tex: [u0, v_bottom] },
            Vertex { pos: [ndc_x + ndc_w, ndc_y], tex: [u1, v_top] },
            Vertex { pos: [ndc_x + ndc_w, ndc_y], tex: [u1, v_top] },
            Vertex { pos: [ndc_x + ndc_w, ndc_y - ndc_h], tex: [u1, v_bottom] },
            Vertex { pos: [ndc_x, ndc_y - ndc_h], tex: [u0, v_bottom] },
        ];

        unsafe {
            // Clear any stale GL errors from prior code.
            while gl::GetError() != gl::NO_ERROR {}

            let scissor = gl::IsEnabled(gl::SCISSOR_TEST);
            if scissor != 0 {
                gl::Disable(gl::SCISSOR_TEST);
            }

            gl::UseProgram(self.program.id());
            gl::Uniform1i(self.u_image_tex, 1);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, texture.id);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * mem::size_of::<Vertex>()) as isize,
                vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );
            gl::DrawArrays(gl::TRIANGLES, 0, vertices.len() as i32);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            gl::UseProgram(0);

            if scissor != 0 {
                gl::Enable(gl::SCISSOR_TEST);
            }
        }
    }

    pub fn remove_image(&mut self, image_id: u32) {
        if let Some(texture) = self.textures.remove(&image_id) {
            unsafe {
                gl::DeleteTextures(1, &texture.id);
            }
        }
    }

    pub fn has_texture(&self, image_id: u32) -> bool {
        self.textures.contains_key(&image_id)
    }

    pub fn cleanup_unused(&mut self, active_ids: &[u32]) {
        let active: HashMap<u32, ()> = active_ids.iter().copied().map(|id| (id, ())).collect();
        let stale: Vec<u32> =
            self.textures.keys().copied().filter(|id| !active.contains_key(id)).collect();
        for id in stale {
            self.remove_image(id);
        }
    }

    pub fn destroy(&mut self) {
        let ids: Vec<u32> = self.textures.keys().copied().collect();
        for id in ids {
            self.remove_image(id);
        }

        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
        }
        self.vbo = 0;
        self.vao = 0;
    }
}

impl Drop for ImageRenderer {
    fn drop(&mut self) {
        self.destroy();
    }
}

fn pixels_to_ndc(x: f32, y: f32, width: f32, height: f32, size_info: &SizeInfo) -> (f32, f32, f32, f32) {
    let half_width = size_info.width() / 2.0;
    let half_height = size_info.height() / 2.0;
    let ndc_x = x / half_width - 1.0;
    let ndc_y = -y / half_height + 1.0;
    let ndc_w = width / half_width;
    let ndc_h = height / half_height;
    (ndc_x, ndc_y, ndc_w, ndc_h)
}
