#version 120
// Lighting for everything opaque: sun and moon with soft shadows, sky
// ambient, block light, ambient occlusion, fog, and the procedural sky.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"
#include "/lib/shadow.glsl"

uniform sampler2D colortex0;   // albedo
uniform sampler2D colortex1;   // normal, block light
uniform sampler2D colortex2;   // sky light, material, ao
uniform sampler2D depthtex0;
uniform sampler2D shadowtex1;  // opaque-only shadow depth
uniform sampler2D noisetex;

uniform mat4 gbufferProjection;
uniform mat4 gbufferProjectionInverse;
uniform mat4 gbufferModelView;
uniform mat4 gbufferModelViewInverse;
uniform mat4 shadowModelView;
uniform mat4 shadowProjection;
uniform vec3 sunPosition;
uniform vec3 moonPosition;
uniform vec3 shadowLightPosition;
uniform float rainStrength;
uniform float viewWidth;
uniform float viewHeight;
uniform float near;
uniform float far;
uniform float frameTimeCounter;
uniform int isEyeInWater;
uniform float nightVision;
uniform float blindness;
uniform ivec2 eyeBrightnessSmooth;

varying vec2 texcoord;

/* DRAWBUFFERS:3 */

const int colortex1Format = RGBA16;
const int colortex2Format = RGBA16;
const int colortex3Format = RGBA16F;
const int colortex4Format = RGBA16F;
const bool colortex3Clear = false;
const float sunPathRotation = -35.0;
const float ambientOcclusionLevel = 0.6;

// Screen-space ambient occlusion: how much of the hemisphere above the
// point is blocked by nearby geometry.
float ambientOcclusion(vec3 viewPos, vec3 normalView, vec2 fragCoord) {
#ifndef AO_ENABLED
    return 1.0;
#else
    float radius = 0.55;
    float occlusion = 0.0;
    float noise = dither(fragCoord) * 6.2831853;
    // Build a basis around the normal.
    vec3 tangent = normalize(cross(normalView, abs(normalView.y) < 0.9 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0)));
    vec3 bitangent = cross(normalView, tangent);
    for (int i = 0; i < 8; i++) {
        float angle = noise + float(i) * 0.7853982;
        float dist = radius * (0.25 + 0.75 * float(i + 1) / 8.0);
        vec3 dir = normalize(tangent * cos(angle) + bitangent * sin(angle) + normalView * 0.9);
        vec3 samplePos = viewPos + dir * dist;
        vec3 screen = viewToScreen(samplePos, gbufferProjection);
        if (screen.x < 0.0 || screen.x > 1.0 || screen.y < 0.0 || screen.y > 1.0) continue;
        float sceneDepth = texture2D(depthtex0, screen.xy).r;
        vec3 sceneView = screenToView(vec3(screen.xy, sceneDepth), gbufferProjectionInverse);
        float diff = sceneView.z - samplePos.z;
        // Blocked if the scene is in front of the sample, but not by a lot
        // (that would be a different object far in front).
        float rangeCheck = smoothstep(0.0, 1.0, radius / max(abs(viewPos.z - sceneView.z), 0.0001));
        occlusion += (diff > 0.02 ? 1.0 : 0.0) * rangeCheck;
    }
    float ao = 1.0 - occlusion / 8.0 * AO_STRENGTH;
    return clamp(ao, 0.0, 1.0);
#endif
}

void main() {
    float depth = texture2D(depthtex0, texcoord).r;
    vec3 viewPos = screenToView(vec3(texcoord, depth), gbufferProjectionInverse);
    vec3 viewDir = normalize(viewPos);
    vec3 sunView = normalize(sunPosition);
    vec3 moonView = normalize(moonPosition);
    vec3 sunWorld = normalize(mat3(gbufferModelViewInverse) * sunView);
    vec3 moonWorld = normalize(mat3(gbufferModelViewInverse) * moonView);
    float day = smoothstep(-0.12, 0.18, sunWorld.y);
    float night = 1.0 - day;
    vec3 sunLight = sunColour(sunWorld.y) * 2.4;
    vec3 moonLight = moonColour() * 0.18;

    // ---- sky ----
    if (depth >= 1.0) {
        vec3 dirWorld = normalize(mat3(gbufferModelViewInverse) * viewDir);
        vec3 sky = skyColour(dirWorld, sunWorld, rainStrength);
        // Sun and moon discs.
        float sunDisc = smoothstep(0.9985, 0.9995, dot(dirWorld, sunWorld));
        sky += sunColour(sunWorld.y) * sunDisc * 12.0 * (1.0 - rainStrength);
        float moonDisc = smoothstep(0.9988, 0.9996, dot(dirWorld, moonWorld));
        sky += vec3(0.9, 0.95, 1.0) * moonDisc * 1.2 * (1.0 - rainStrength);
        // Stars: a hash on a coarse grid of directions, only at night.
        vec3 starDir = dirWorld * 180.0;
        float star = step(0.9985, hash13(floor(starDir))) * night * (1.0 - rainStrength);
        star *= smoothstep(0.02, 0.2, dirWorld.y);
        sky += vec3(star) * (0.6 + 0.4 * sin(frameTimeCounter * 3.0 + hash13(floor(starDir) + 1.0) * 6.28));
        gl_FragData[0] = vec4(sky, 1.0);
        return;
    }

    // ---- surface data ----
    vec3 albedo = texture2D(colortex0, texcoord).rgb;
    vec4 data1 = texture2D(colortex1, texcoord);
    vec4 data2 = texture2D(colortex2, texcoord);
    vec3 normalWorld = decodeNormal(data1.xyz);
    vec3 normalView = normalize(mat3(gbufferModelView) * normalWorld);
    float blockLight = data1.w;
    float skyLight = data2.x;
    float material = data2.y * 10.0;
    vec3 worldPos = (gbufferModelViewInverse * vec4(viewPos, 1.0)).xyz; // relative to the camera
    vec2 fragCoord = texcoord * vec2(viewWidth, viewHeight);

    // Foliage is thin: light it from both sides.
    float ndotl = dot(normalWorld, sunWorld);
    float ndotlMoon = dot(normalWorld, moonWorld);
    if (material > 0.5 && material < 1.5) {
        ndotl = abs(ndotl) * 0.75 + 0.25;
        ndotlMoon = abs(ndotlMoon) * 0.75 + 0.25;
    }
    ndotl = clamp(ndotl, 0.0, 1.0);
    ndotlMoon = clamp(ndotlMoon, 0.0, 1.0);

    // ---- shadows ----
    float shadow = 1.0;
#ifdef SHADOWS_ENABLED
    // Sky light acts as a cheap shadow for caves, where the shadow map
    // never reaches; the map itself handles the surface.
    vec3 shadowLightWorld = normalize(mat3(gbufferModelViewInverse) * normalize(shadowLightPosition));
    shadow = sampleShadow(shadowtex1, shadowModelView, shadowProjection, worldPos, normalWorld, shadowLightWorld, fragCoord, SHADOW_SOFTNESS);
    // Fade shadows out at the edge of the shadow map distance.
    float distFade = smoothstep(120.0, 160.0, length(worldPos));
    shadow = mix(shadow, 1.0, distFade);
#endif
    shadow *= smoothstep(0.1, 0.6, skyLight);
    shadow = mix(shadow, shadow * 0.35, rainStrength);

    // ---- ambient ----
    float ao = ambientOcclusion(viewPos, normalView, fragCoord) * mix(1.0, data2.z, 0.5);
    vec3 skyAmbient = mix(skyColour(vec3(0.0, 1.0, 0.0), sunWorld, rainStrength), skyColour(normalWorld, sunWorld, rainStrength), 0.5);
    skyAmbient *= 0.9 + 0.3 * clamp(normalWorld.y, 0.0, 1.0);
    float skyMask = pow(skyLight, 1.6);
    vec3 ambient = skyAmbient * skyMask * mix(0.55, 0.9, day);
    ambient += vec3(0.012, 0.014, 0.02); // never pitch black

    // Torches and other block light, warm and falling off with distance.
    float blockMask = pow(blockLight, 2.2);
    vec3 blockColour = vec3(1.0, 0.62, 0.32);
    vec3 block = blockColour * blockMask * 1.6;

    // ---- combine ----
    vec3 direct = sunLight * ndotl * shadow * day + moonLight * ndotlMoon * shadow * night;
    direct *= 1.0 - rainStrength * 0.7;
    vec3 lighting = (direct + ambient) * ao + block * mix(ao, 1.0, 0.6);
    lighting += vec3(nightVision) * 0.6;
    vec3 colour = albedo * lighting;
    if (material > 2.5) {
        // Emissive blocks glow on their own.
        colour += albedo * albedo * 2.2;
    }

    // ---- fog ----
    float dist = length(viewPos);
    vec3 dirWorld = normalize(worldPos);
    vec3 fogTint = skyColour(vec3(dirWorld.x, max(dirWorld.y, 0.05), dirWorld.z), sunWorld, rainStrength);
    float density = 0.6 + rainStrength * 1.8;
    colour = applyFog(colour, dist, fogTint, density, 0.35, far);
    if (isEyeInWater == 1) {
        colour = mix(colour, vec3(0.02, 0.12, 0.2) * (0.3 + 0.7 * day), clamp(dist / 24.0, 0.0, 0.92));
    } else if (isEyeInWater == 2) {
        colour = mix(colour, vec3(0.6, 0.12, 0.02), clamp(dist / 3.0, 0.0, 0.98));
    }
    colour *= 1.0 - blindness;

    gl_FragData[0] = vec4(colour, 1.0);
}
