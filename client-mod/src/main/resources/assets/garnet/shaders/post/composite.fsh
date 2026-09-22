#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 4: apply the lighting to the frame, shade water surfaces, then add
// atmosphere: aerial perspective that takes on the sun's colour, and light
// shafts where the sky shows through between things.

#include <garnet:frame.glsl>

uniform sampler2D SceneSampler;
uniform sampler2D DepthSampler;
uniform sampler2D LightSampler;
uniform sampler2D TerrainMapSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 SceneSize;
    vec2 DepthSize;
    vec2 LightSize;
    vec2 TerrainMapSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

const int SHAFT_SAMPLES = 24;
const int REFLECTION_STEPS = 28;

// How much of the sky is visible along the line from this pixel to the sun.
float lightShafts(vec2 sunUv, float noise) {
    vec2 delta = (sunUv - texCoord) / float(SHAFT_SAMPLES);
    // Don't reach too far across the screen; keeps the effect local.
    delta *= 0.85;
    vec2 uv = texCoord + delta * noise;
    float light = 0.0;
    float weight = 1.0;
    float decay = 0.94;
    for (int i = 0; i < SHAFT_SAMPLES; i++) {
        uv += delta;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) break;
        float d = texture(DepthSampler, uv).r;
        light += (isSky(d) ? 1.0 : 0.0) * weight;
        weight *= decay;
    }
    return light / float(SHAFT_SAMPLES);
}

// Small rolling waves: a few sines at different scales drifting with time.
vec3 waveNormal(vec2 xz, float time) {
    float t = time * 0.9;
    float sx = cos(xz.x * 1.7 + t * 1.3) * 0.85
             + cos((xz.x + xz.y) * 4.1 + t * 2.1) * 1.4
             + cos((xz.x - xz.y * 0.6) * 5.3 - t * 1.7) * 1.3;
    float sz = cos(xz.y * 2.3 - t * 1.1) * 1.15
             + cos((xz.x + xz.y) * 4.1 + t * 2.1) * 1.4
             - cos((xz.x - xz.y * 0.6) * 5.3 - t * 1.7) * 0.8;
    return normalize(vec3(-sx * 0.035, 1.0, -sz * 0.035));
}

// March a reflected ray through the depth buffer; the hit uv, or -1 for none.
vec2 reflectionHit(vec3 origin, vec3 dir, float noise) {
    vec3 ray = origin;
    vec3 step = dir * 0.3;
    ray += step * noise;
    for (int i = 0; i < REFLECTION_STEPS; i++) {
        ray += step;
        step *= 1.15;
        vec3 s = project(ray);
        if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0 || s.z < 0.0) return vec2(-1.0);
        float d = texture(DepthSampler, s.xy).r;
        if (isSky(d)) continue;
        vec3 scene = viewPosition(s.xy, d);
        float dz = scene.z - ray.z;
        if (dz > 0.02 && dz < length(step) * 2.5 + 0.3) return s.xy;
    }
    return vec2(-1.0);
}

vec3 shadeWater(vec3 colour, vec3 p, vec3 worldRel, float sun, float noise, vec3 sunCol) {
    float daylight = SunDirView.w;
    float rain = FogParams.z;
    float depthW = mapWaterDepth(TerrainMapSampler, worldRel.xz);
    vec3 nWorld = waveNormal(worldRel.xz, CameraPos.w);
    vec3 nView = normalize((ViewMat * vec4(nWorld, 0.0)).xyz);
    vec3 viewDir = normalize(p);

    // Fresnel: glancing views are mirrors, looking straight down sees through.
    float cosTheta = clamp(dot(-viewDir, nView), 0.0, 1.0);
    float fresnel = 0.02 + 0.98 * pow(1.0 - cosTheta, 5.0);

    // Reflection: the scene where the ray lands, otherwise the sky.
    vec3 reflectDir = reflect(viewDir, nView);
    vec3 reflectWorld = normalize((ViewInv * vec4(reflectDir, 0.0)).xyz);
    float sunAmount = pow(max(dot(reflectWorld, normalize(SunDirWorld.xyz)), 0.0), 8.0);
    vec3 sky = mix(SkyColor.rgb * 1.05, sunCol, sunAmount * daylight * 0.5) * mix(0.25, 1.0, daylight);
    sky = mix(sky, vec3(0.5, 0.53, 0.58) * mix(0.3, 1.0, daylight), rain * 0.7);
    vec3 reflection = sky;
    vec2 hit = reflectionHit(p + nView * 0.05, reflectDir, noise);
    if (hit.x >= 0.0) {
        vec2 edge = smoothstep(0.0, 0.15, hit) * smoothstep(0.0, 0.15, 1.0 - hit);
        reflection = mix(sky, texture(SceneSampler, hit).rgb, edge.x * edge.y);
    }

    // The water body: what vanilla drew, pulled towards deep blue-green with depth.
    vec3 deep = vec3(0.02, 0.09, 0.14) * mix(0.15, 1.0, daylight);
    float absorb = 1.0 - exp(-depthW * 0.28);
    vec3 body = mix(colour, deep, absorb * 0.75);

    // Sun glint on the waves.
    vec3 halfVec = normalize(normalize(SunDirView.xyz) - viewDir);
    float glint = pow(max(dot(nView, halfVec), 0.0), 320.0) * daylight * sun * (1.0 - rain);

    vec3 shaded = mix(body, reflection, clamp(fresnel * 0.95 + 0.05, 0.0, 1.0)) + sunCol * glint * 1.6;
    return mix(colour, shaded, MapParams.w);
}

void main() {
    vec3 colour = texture(SceneSampler, texCoord).rgb;
    float depth = texture(DepthSampler, texCoord).r;
    float daylight = SunDirView.w;
    float rain = FogParams.z;
    bool underwater = FogParams.w > 0.5;
    float noise = interleavedNoise(gl_FragCoord.xy);
    vec3 sunCol = sunColour();

    if (!isSky(depth) && !underwater) {
        vec3 light = texture(LightSampler, texCoord).rgb;
        float ao = light.r;
        float sun = light.g;
        bool water = light.b > 0.5 && MapParams.w > 0.0;

        vec3 p = viewPosition(texCoord, depth);
        if (water) {
            colour = shadeWater(colour, p, worldRelative(p), sun, noise, sunCol);
            ao = 1.0;
        }

        // Shadowed areas keep the sky's ambient light; lit areas get the sun.
        float shade = mix(1.0, sun, Strength.y * daylight * (1.0 - rain * 0.8));
        vec3 shadowTint = mix(vec3(0.62, 0.68, 0.80), vec3(1.0), shade);
        colour *= shadowTint;
        // Lit surfaces get a touch of the sun's colour; kept subtle so the
        // vanilla palette stays recognisable.
        vec3 warm = mix(vec3(1.0), normalize(sunCol) * 1.7, 0.06);
        colour *= mix(vec3(1.0), warm, daylight * shade);
        colour *= mix(1.0, ao, 0.9);

        // Aerial perspective: distance haze in the sky's colour, glowing near the sun.
        float dist = length(p);
        vec3 viewDir = normalize(p);
        float sunAmount = pow(max(dot(viewDir, normalize(SunDirView.xyz)), 0.0), 6.0);
        vec3 haze = mix(SkyColor.rgb, sunCol, sunAmount * daylight * 0.6);
        haze = mix(haze, vec3(0.55, 0.58, 0.62), rain * 0.7);
        float density = (0.0022 + rain * 0.006) * mix(1.6, 1.0, daylight);
        float fog = 1.0 - exp(-dist * density);
        fog *= smoothstep(0.0, 1.0, dist / max(FogParams.x * 0.9, 1.0)) * 0.9 + 0.1;
        colour = mix(colour, haze, clamp(fog, 0.0, 0.85));
    }

    // Light shafts: only when the sun is on screen, in front of the camera.
    if (Misc.w > 0.0 && daylight > 0.0 && !underwater) {
        vec3 sunView = normalize(SunDirView.xyz) * 500.0;
        vec3 s = project(sunView);
        if (s.x >= -0.3 && s.x <= 1.3 && s.y >= -0.3 && s.y <= 1.3) {
            float shafts = lightShafts(s.xy, noise);
            vec2 centred = s.xy - 0.5;
            float onScreen = 1.0 - smoothstep(0.55, 0.9, length(centred));
            colour += sunCol * shafts * 0.22 * Misc.w * daylight * (1.0 - rain) * onScreen;
        }
    }

    fragColor = vec4(colour, 1.0);
}
