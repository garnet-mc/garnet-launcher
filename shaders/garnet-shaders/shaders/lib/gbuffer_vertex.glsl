// Opaque terrain: passes surface data to the G-buffer and waves plants.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"

attribute vec4 mc_Entity;
attribute vec4 mc_midTexCoord;

uniform mat4 gbufferModelViewInverse;
uniform vec3 cameraPosition;
uniform float frameTimeCounter;
uniform float rainStrength;

varying vec2 texcoord;
varying vec2 lmcoord;
varying vec4 vcolor;
varying vec3 normalWorld;
varying float materialId;

// A gentle wind: two sines at different frequencies plus a gust term.
vec3 wind(vec3 worldPos, float strength) {
    float t = frameTimeCounter;
    float gust = fbm2(worldPos.xz * 0.05 + t * 0.15) * 2.0 - 1.0;
    float sway = sin(worldPos.x * 1.7 + t * 1.9) * 0.5 + sin(worldPos.z * 1.3 + t * 1.4) * 0.35 + gust * 0.6;
    float rain = 1.0 + rainStrength * 1.5;
    return vec3(sway, 0.0, sin(worldPos.z * 1.1 + t * 1.7) * 0.3) * 0.08 * strength * rain;
}

void main() {
    texcoord = (gl_TextureMatrix[0] * gl_MultiTexCoord0).xy;
    lmcoord = (gl_TextureMatrix[1] * gl_MultiTexCoord1).xy;
    vcolor = gl_Color;
    normalWorld = normalize(mat3(gbufferModelViewInverse) * (gl_NormalMatrix * gl_Normal));
    materialId = mc_Entity.x;

    vec4 viewPos = gl_ModelViewMatrix * gl_Vertex;
    vec3 worldPos = (gbufferModelViewInverse * viewPos).xyz + cameraPosition;

#ifdef WAVING_PLANTS
    bool plant = mc_Entity.x == 10000.0;
    bool tallPlant = mc_Entity.x == 10004.0;
    bool leaves = mc_Entity.x == 10001.0;
    // Only the top of a plant moves; the texture's mid point tells us which
    // vertices are above the block's centre.
    bool topVertex = gl_MultiTexCoord0.t < mc_midTexCoord.t;
    if ((plant && topVertex) || tallPlant) {
        viewPos.xyz += mat3(gl_ModelViewMatrix) * wind(worldPos, tallPlant ? 1.4 : 1.0);
    } else if (leaves) {
        viewPos.xyz += mat3(gl_ModelViewMatrix) * wind(worldPos, 0.45);
    }
#endif

    gl_Position = gl_ProjectionMatrix * viewPos;
}
