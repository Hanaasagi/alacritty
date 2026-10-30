precision mediump float;

varying vec2 TexCoord;

uniform sampler2D imageTex;

void main() {
    gl_FragColor = texture2D(imageTex, TexCoord);
}
