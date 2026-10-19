#if defined(GLES2_RENDERER)
// GLES2 does not support gl_VertexID; post-processing is GLSL3-only.
#else
layout(location = 0) in vec2 dummyPos;
out vec2 TexCoords;

void main() {
    // Generate fullscreen triangle from gl_VertexID (dummyPos is unused but
    // the attribute must exist in the VAO for macOS driver compatibility).
    float x = -1.0 + float((gl_VertexID & 1) << 2);
    float y = -1.0 + float((gl_VertexID & 2) << 1);
    TexCoords = vec2((x + 1.0) * 0.5, (y + 1.0) * 0.5);
    gl_Position = vec4(x, y, 0.0, 1.0) + vec4(dummyPos * 0.0, 0.0, 0.0);
}
#endif
