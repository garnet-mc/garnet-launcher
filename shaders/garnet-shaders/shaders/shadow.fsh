#version 120
// Cut-out blocks (leaves, grass) must not cast solid square shadows.

uniform sampler2D texture;

varying vec2 texcoord;
varying vec4 vcolor;

void main() {
    vec4 albedo = texture2D(texture, texcoord) * vcolor;
    if (albedo.a < 0.1) discard;
    gl_FragData[0] = albedo;
}
