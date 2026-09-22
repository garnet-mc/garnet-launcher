#version 120
// Bloom, at half resolution: keep the bright parts, blur them.

#include "/lib/settings.glsl"

uniform sampler2D colortex3;
uniform float viewWidth;
uniform float viewHeight;

varying vec2 texcoord;

/* DRAWBUFFERS:4 */

vec3 bright(vec2 uv) {
    vec3 c = texture2D(colortex3, uv).rgb;
    float lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
    // Soft knee so bloom fades in rather than switching on.
    float k = smoothstep(0.85, 1.6, lum);
    return c * k;
}

void main() {
#ifndef BLOOM_ENABLED
    gl_FragData[0] = vec4(0.0);
    return;
#else
    // The pass runs at half resolution, so one texel here is two of the scene.
    vec2 texel = 2.0 / vec2(viewWidth, viewHeight);
    vec3 sum = vec3(0.0);
    float total = 0.0;
    // A 5x5 Gaussian-ish tap pattern stretched over a wide radius.
    for (int y = -2; y <= 2; y++) {
        for (int x = -2; x <= 2; x++) {
            vec2 offset = vec2(float(x), float(y)) * texel * 3.0;
            float w = exp(-(float(x * x + y * y)) / 5.0);
            sum += bright(texcoord + offset) * w;
            total += w;
        }
    }
    gl_FragData[0] = vec4(sum / total, 1.0);
#endif
}
