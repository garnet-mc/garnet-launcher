#version 120
// Water and other translucent blocks: animated waves, Fresnel, screen-space
// reflections of the already-lit opaque scene, sun glints and absorption.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"
#include "/lib/shadow.glsl"

uniform sampler2D texture;
uniform sampler2D colortex3;   // lit opaque scene
uniform sampler2D depthtex1;   // opaque depth (translucents excluded)
uniform sampler2D shadowtex1;

uniform mat4 gbufferProjection;
uniform mat4 gbufferProjectionInverse;
uniform mat4 gbufferModelView;
uniform mat4 gbufferModelViewInverse;
uniform mat4 shadowModelView;
uniform mat4 shadowProjection;
uniform vec3 sunPosition;
uniform vec3 moonPosition;
uniform vec3 shadowLightPosition;
uniform vec3 cameraPosition;
uniform float frameTimeCounter;
uniform float rainStrength;
uniform float viewWidth;
uniform float viewHeight;
uniform float far;
uniform int isEyeInWater;

varying vec2 texcoord;
varying vec2 lmcoord;
varying vec4 vcolor;
varying vec3 normalWorld;
varying vec3 worldPos;
varying vec3 viewPos;
varying float isWater;

/* DRAWBUFFERS:3 */

// Height of the water surface at a point: a few octaves of scrolling noise.
float waveHeight(vec2 p) {
    float t = frameTimeCounter;
    float h = 0.0;
    h += noise2(p * 0.9 + vec2(t * 0.35, t * 0.2)) * 0.5;
    h += noise2(p * 2.1 - vec2(t * 0.25, t * 0.4)) * 0.3;
    h += noise2(p * 4.7 + vec2(t * 0.6, -t * 0.3)) * 0.15;
    h += noise2(p * 9.0 - vec2(t * 0.9, t * 0.7)) * 0.05;
    return h * WATER_WAVES;
}

vec3 waveNormal(vec2 p) {
    float e = 0.05;
    float h = waveHeight(p);
    float hx = waveHeight(p + vec2(e, 0.0));
    float hz = waveHeight(p + vec2(0.0, e));
    // Wave amplitude in blocks is small; scale the slope to taste.
    vec3 n = normalize(vec3(-(hx - h) * 0.9, e, -(hz - h) * 0.9));
    return n;
}

// Marches a reflected ray through the depth buffer of the opaque scene.
vec3 screenSpaceReflection(vec3 viewPosition, vec3 reflectDirView, vec3 fallback, vec2 fragCoord) {
#ifndef WATER_REFLECTIONS
    return fallback;
#else
    vec3 rayPos = viewPosition;
    vec3 rayStep = reflectDirView * 0.25;
    float jitter = dither(fragCoord);
    rayPos += rayStep * jitter;
    vec3 hit = vec3(-1.0);
    for (int i = 0; i < 40; i++) {
        rayPos += rayStep;
        rayStep *= 1.12; // longer steps as we go
        vec3 screen = viewToScreen(rayPos, gbufferProjection);
        if (screen.x < 0.0 || screen.x > 1.0 || screen.y < 0.0 || screen.y > 1.0 || screen.z > 1.0) break;
        float sceneDepth = texture2D(depthtex1, screen.xy).r;
        vec3 sceneView = screenToView(vec3(screen.xy, sceneDepth), gbufferProjectionInverse);
        float thickness = abs(rayStep.z) * 2.0 + 0.2;
        if (sceneView.z > rayPos.z && sceneView.z - rayPos.z < thickness) {
            hit = screen;
            break;
        }
    }
    if (hit.x < 0.0) return fallback;
    // Fade near the screen edges so the cut-off is not a hard line.
    vec2 edge = smoothstep(0.0, 0.12, hit.xy) * smoothstep(0.0, 0.12, 1.0 - hit.xy);
    float fade = edge.x * edge.y;
    return mix(fallback, texture2D(colortex3, hit.xy).rgb, fade);
#endif
}

void main() {
    vec4 albedo = texture2D(texture, texcoord) * vcolor;
    vec2 fragCoord = gl_FragCoord.xy;

    vec3 sunView = normalize(sunPosition);
    vec3 sunWorld = normalize(mat3(gbufferModelViewInverse) * sunView);
    vec3 moonWorld = normalize(mat3(gbufferModelViewInverse) * normalize(moonPosition));
    float day = smoothstep(-0.12, 0.18, sunWorld.y);
    vec3 relPos = worldPos - cameraPosition;

    vec3 n = normalWorld;
    if (isWater > 0.5 && abs(normalWorld.y) > 0.5) {
        n = waveNormal(worldPos.xz * 0.5);
        if (normalWorld.y < 0.0) n = -n;
    }
    vec3 nView = normalize(mat3(gbufferModelView) * n);
    vec3 viewDir = normalize(viewPos);

    // Fresnel: more reflective at grazing angles.
    float cosTheta = clamp(dot(-viewDir, nView), 0.0, 1.0);
    float f0 = isWater > 0.5 ? 0.02 : 0.04;
    float fresnel = f0 + (1.0 - f0) * pow(1.0 - cosTheta, 5.0);
    if (isEyeInWater == 1) fresnel *= 0.3;

    // Reflection: the scene, falling back to the sky.
    vec3 reflectView = reflect(viewDir, nView);
    vec3 reflectWorld = normalize(mat3(gbufferModelViewInverse) * reflectView);
    vec3 skyReflection = skyColour(vec3(reflectWorld.x, max(reflectWorld.y, 0.02), reflectWorld.z), sunWorld, rainStrength);
    vec3 reflection = screenSpaceReflection(viewPos, reflectView, skyReflection, fragCoord);

    // Lighting for the water surface itself.
    vec3 shadowLightWorld = normalize(mat3(gbufferModelViewInverse) * normalize(shadowLightPosition));
    float shadow = 1.0;
#ifdef SHADOWS_ENABLED
    shadow = sampleShadow(shadowtex1, shadowModelView, shadowProjection, relPos, normalWorld, shadowLightWorld, fragCoord, SHADOW_SOFTNESS);
#endif
    shadow *= smoothstep(0.1, 0.6, lmcoord.y);
    vec3 sunLight = sunColour(sunWorld.y) * 2.4 * day * (1.0 - rainStrength * 0.7);
    vec3 ambient = skyColour(vec3(0.0, 1.0, 0.0), sunWorld, rainStrength) * pow(lmcoord.y, 1.6) * mix(0.55, 0.9, day) + vec3(0.012, 0.014, 0.02);
    vec3 block = vec3(1.0, 0.62, 0.32) * pow(lmcoord.x, 2.2) * 1.6;
    float ndotl = clamp(dot(n, sunWorld), 0.0, 1.0);
    vec3 lighting = sunLight * ndotl * shadow + ambient + block;

    // Sun glint: a sharp specular highlight on the waves.
    vec3 halfVec = normalize(sunView - viewDir);
    float spec = pow(clamp(dot(nView, halfVec), 0.0, 1.0), isWater > 0.5 ? 220.0 : 60.0);
    vec3 glint = sunColour(sunWorld.y) * spec * shadow * day * (isWater > 0.5 ? 3.0 : 0.8) * (1.0 - rainStrength);

    vec3 colour;
    float alpha;
    if (isWater > 0.5) {
        // Deep water is darker; the vertex colour carries the biome tint.
        vec3 waterTint = vcolor.rgb * vec3(0.55, 0.8, 1.0);
        vec3 body = waterTint * lighting * 0.35;
        colour = mix(body, reflection, fresnel * 0.85 + 0.08) + glint;
        alpha = clamp(0.62 + fresnel * 0.35, 0.0, 0.94);
    } else {
        colour = albedo.rgb * lighting;
        colour = mix(colour, reflection, fresnel * 0.6) + glint * albedo.a;
        alpha = albedo.a;
    }

    // Fog, same as the opaque pass so translucents blend in.
    float dist = length(viewPos);
    vec3 dirWorld = normalize(relPos);
    vec3 fogTint = skyColour(vec3(dirWorld.x, max(dirWorld.y, 0.05), dirWorld.z), sunWorld, rainStrength);
    colour = applyFog(colour, dist, fogTint, 0.6 + rainStrength * 1.8, 0.35, far);

    gl_FragData[0] = vec4(colour, alpha);
}
