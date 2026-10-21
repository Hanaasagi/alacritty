#version 330 core

// ShaderToy standard uniforms
uniform vec3 iResolution;
uniform float iTime;
uniform float iTimeDelta;
uniform float iFrameRate;
uniform int iFrame;
uniform vec4 iMouse;
uniform vec4 iDate;
uniform vec3 iChannelResolution[4];

// Ghostty cursor uniforms (cursor-shaders)
uniform vec4 iCurrentCursor;
uniform vec4 iPreviousCursor;
uniform vec4 iCurrentCursorColor;
uniform vec4 iPreviousCursorColor;
uniform int iCurrentCursorStyle;
uniform int iPreviousCursorStyle;
uniform int iCursorVisible;
uniform float iTimeCursorChange;
uniform float iTimeFocus;
uniform int iFocus;

// Ghostty color uniforms (declared for compatibility)
uniform vec3 iBackgroundColor;
uniform vec3 iForegroundColor;
uniform vec3 iCursorColor;
uniform vec3 iCursorText;
uniform vec3 iSelectionForegroundColor;
uniform vec3 iSelectionBackgroundColor;
uniform vec3 iPalette[256];

uniform sampler2D iChannel0;

#define CURSORSTYLE_BLOCK 0
#define CURSORSTYLE_BLOCK_HOLLOW 1
#define CURSORSTYLE_BAR 2
#define CURSORSTYLE_UNDERLINE 3
#define CURSORSTYLE_LOCK 4

#define texture2D texture

out vec4 _fragColor;

void mainImage(out vec4 fragColor, in vec2 fragCoord);
void _alacritty_init_globals();
void main() { _alacritty_init_globals(); mainImage(_fragColor, gl_FragCoord.xy); }
