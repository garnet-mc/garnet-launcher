#version 120
// Shadow pass: the world as seen from the sun, with the same distortion the
// lighting passes undo when they sample it.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"
#include "/lib/shadow.glsl"

attribute vec4 mc_Entity;

varying vec2 texcoord;
varying vec4 vcolor;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    vcolor = gl_Color;
    vec4 clip = ftransform();
    clip.xyz = distortShadow(clip.xyz / clip.w) * clip.w;
    gl_Position = clip;
}
