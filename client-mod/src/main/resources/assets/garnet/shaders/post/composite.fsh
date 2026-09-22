#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 4: apply the lighting to the frame, then add atmosphere: aerial
// perspective that takes on the sun's colour, and light shafts where the
// sky shows through between things.

#include <garnet:frame.glsl>

uniform sampler2D SceneSampler;
uniform sampler2D DepthSampler;
uniform sampler2D LightSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 SceneSize;
    vec2 DepthSize;
    vec2 LightSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

const int SHAFT_SAMPLES = 24;

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

void main() {
    vec3 colour = texture(SceneSampler, texCoord).rgb;
    float depth = texture(DepthSampler, texCoord).r;
    float daylight = SunDirView.w;
    float rain = FogParams.z;
    bool underwater = FogParams.w > 0.5;
    float noise = interleavedNoise(gl_FragCoord.xy);
    vec3 sunCol = sunColour();

    if (!isSky(depth) && !underwater) {
        vec2 light = texture(LightSampler, texCoord).rg;
        float ao = light.r;
        float sun = light.g;

        // Shadowed areas keep the sky's ambient light; lit areas get the sun.
        float shade = mix(1.0, sun, Strength.y * daylight * (1.0 - rain * 0.8));
        vec3 shadowTint = mix(vec3(0.62, 0.68, 0.80), vec3(1.0), shade);
        colour *= shadowTint;
        // Lit surfaces get a touch of the sun's colour; kept subtle so the
        // vanilla palette stays recognisable.
        vec3 warm = mix(vec3(1.0), normalize(sunCol) * 1.7, 0.10);
        colour *= mix(vec3(1.0), warm, daylight * shade);
        colour *= mix(1.0, ao, 0.9);

        // Aerial perspective: distance haze in the sky's colour, glowing near the sun.
        vec3 p = viewPosition(texCoord, depth);
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
