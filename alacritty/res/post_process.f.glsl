#if defined(GLES2_RENDERER)
// GLES2 不支持后处理
#else
in vec2 TexCoords;
out vec4 FragColor;

uniform sampler2D screenTexture;

void main() {
    FragColor = texture(screenTexture, TexCoords);
}
#endif
