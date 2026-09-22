#version 120
// Translucent blocks (water, glass, ice): lit right here, in one pass.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"

attribute vec4 mc_Entity;

uniform mat4 gbufferModelViewInverse;
uniform vec3 cameraPosition;

varying vec2 texcoord;
varying vec2 lmcoord;
varying vec4 vcolor;
varying vec3 normalWorld;
varying vec3 worldPos;
varying vec3 viewPos;
varying float isWater;

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    lmcoord = (gl_TextureMatrix[1] * gl_MultiTexCoord1).xy;
    vcolor = gl_Color;
    normalWorld = normalize(mat3(gbufferModelViewInverse) * (gl_NormalMatrix * gl_Normal));
    isWater = mc_Entity.x == 10002.0 ? 1.0 : 0.0;
    vec4 view = gl_ModelViewMatrix * gl_Vertex;
    viewPos = view.xyz;
    worldPos = (gbufferModelViewInverse * view).xyz + cameraPosition;
    gl_Position = gl_ProjectionMatrix * view;
}
