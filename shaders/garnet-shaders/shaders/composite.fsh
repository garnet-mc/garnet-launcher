#version 120
// After everything is drawn: volumetric light shafts and, optionally,
// screen-space ray-traced global illumination.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"
#include "/lib/shadow.glsl"

uniform sampler2D colortex0;
uniform sampler2D colortex1;
uniform sampler2D colortex2;
uniform sampler2D colortex3;
uniform sampler2D depthtex0;
uniform sampler2D shadowtex0;

uniform mat4 gbufferProjection;
uniform mat4 gbufferProjectionInverse;
uniform mat4 gbufferModelView;
uniform mat4 gbufferModelViewInverse;
uniform mat4 shadowModelView;
uniform mat4 shadowProjection;
uniform vec3 sunPosition;
uniform vec3 shadowLightPosition;
uniform float rainStrength;
uniform float viewWidth;
uniform float viewHeight;
uniform float far;
uniform float frameTimeCounter;
uniform int isEyeInWater;
uniform ivec2 eyeBrightnessSmooth;

varying vec2 texcoord;

/* DRAWBUFFERS:3 */

// Light scattered toward the camera along the view ray: march from the eye
// to the surface and count how many steps are in sunlight.
vec3 volumetricLight(vec3 viewPos, vec3 sunWorld, vec2 fragCoord) {
#ifndef VOLUMETRIC_LIGHT
    return vec3(0.0);
#else
    const int steps = 14;
    float day = smoothstep(-0.12, 0.18, sunWorld.y);
    if (day < 0.01) return vec3(0.0);
    vec3 end = (gbufferModelViewInverse * vec4(viewPos, 1.0)).xyz;
    float dist = min(length(end), 96.0);
    vec3 dir = normalize(end);
    float jitter = dither(fragCoord);
    float lit = 0.0;
    vec3 shadowLightWorld = normalize(mat3(gbufferModelViewInverse) * normalize(shadowLightPosition));
    for (int i = 0; i < steps; i++) {
        float t = (float(i) + jitter) / float(steps);
        vec3 p = dir * dist * t;
        vec4 shadowClip = shadowProjection * (shadowModelView * vec4(p, 1.0));
        vec3 ndc = shadowClip.xyz / shadowClip.w;
        vec3 s = distortShadow(ndc) * 0.5 + 0.5;
        if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0) {
            lit += 1.0;
            continue;
        }
        lit += step(s.z - 0.0008, texture2D(shadowtex0, s.xy).r);
    }
    lit /= float(steps);
    // Stronger when looking toward the sun, and only outdoors.
    vec3 viewDir = normalize(viewPos);
    float towardSun = pow(clamp(dot(viewDir, normalize(sunPosition)), 0.0, 1.0), 4.0);
    float outdoors = clamp(float(eyeBrightnessSmooth.y) / 240.0, 0.0, 1.0);
    float amount = lit * (0.12 + towardSun * 0.9) * (dist / 96.0) * outdoors * day * VL_STRENGTH;
    amount *= 1.0 + rainStrength * 0.5;
    return sunColour(sunWorld.y) * amount * 0.7;
#endif
}

// Screen-space GI: shoot a few rays from the surface, and where they hit
// something on screen, borrow its lit colour as bounced light.
vec3 rayTracedGI(vec3 viewPos, vec3 normalView, vec2 fragCoord) {
#ifndef RTGI_ENABLED
    return vec3(0.0);
#else
    vec3 tangent = normalize(cross(normalView, abs(normalView.y) < 0.9 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0)));
    vec3 bitangent = cross(normalView, tangent);
    vec3 gi = vec3(0.0);
    float noise = dither(fragCoord);
    for (int r = 0; r < RTGI_SAMPLES; r++) {
        // Cosine-weighted direction on the hemisphere.
        float u = fract(noise + float(r) * 0.618034);
        float v = fract(noise * 7.13 + float(r) * 0.381966);
        float phi = u * 6.2831853;
        float cosT = sqrt(1.0 - v);
        float sinT = sqrt(v);
        vec3 dir = tangent * cos(phi) * sinT + bitangent * sin(phi) * sinT + normalView * cosT;
        vec3 p = viewPos + normalView * 0.05;
        vec3 stepv = dir * 0.3;
        for (int i = 0; i < 12; i++) {
            p += stepv;
            stepv *= 1.15;
            vec3 s = viewToScreen(p, gbufferProjection);
            if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0) break;
            float d = texture2D(depthtex0, s.xy).r;
            if (d >= 1.0) break;
            vec3 scene = screenToView(vec3(s.xy, d), gbufferProjectionInverse);
            float diff = scene.z - p.z;
            if (diff > 0.0 && diff < 0.6) {
                // Hit: light arriving from that surface.
                vec3 hitNormal = normalize(mat3(gbufferModelView) * decodeNormal(texture2D(colortex1, s.xy).xyz));
                float facing = clamp(dot(hitNormal, -dir), 0.0, 1.0);
                gi += texture2D(colortex3, s.xy).rgb * facing;
                break;
            }
        }
    }
    return gi / float(RTGI_SAMPLES);
#endif
}

void main() {
    vec3 colour = texture2D(colortex3, texcoord).rgb;
    float depth = texture2D(depthtex0, texcoord).r;
    vec3 viewPos = screenToView(vec3(texcoord, depth), gbufferProjectionInverse);
    vec3 sunWorld = normalize(mat3(gbufferModelViewInverse) * normalize(sunPosition));
    vec2 fragCoord = texcoord * vec2(viewWidth, viewHeight);

    if (depth < 1.0) {
        vec3 albedo = texture2D(colortex0, texcoord).rgb;
        vec3 normalView = normalize(mat3(gbufferModelView) * decodeNormal(texture2D(colortex1, texcoord).xyz));
        colour += albedo * rayTracedGI(viewPos, normalView, fragCoord) * 0.9;
    }

    if (isEyeInWater == 0) {
        colour += volumetricLight(viewPos, sunWorld, fragCoord);
    }

    gl_FragData[0] = vec4(colour, 1.0);
}
