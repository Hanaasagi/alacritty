in vec2 TexCoord;
out vec4 FragColor;

uniform sampler2D imageTex;

void main() {
    FragColor = texture(imageTex, TexCoord);
}
