#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 8: add bloom, expose, tone map, vignette, and a light sharpen. Writes
// the finished frame back to the main target.

#include <garnet:frame.glsl>

uniform sampler2D SceneSampler;
uniform sampler2D BloomSampler;
uniform sampler2D DepthSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 SceneSize;
    vec2 BloomSize;
    vec2 DepthSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

// Narkowicz's ACES fit: highlights roll off instead of clipping.
vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec3 colour = texture(SceneSampler, texCoord).rgb;

    // Unsharp mask: bring back the crispness the blurs and haze soften.
    vec2 texel = 1.0 / SceneSize;
    vec3 around = texture(SceneSampler, texCoord + vec2(texel.x, 0.0)).rgb
                + texture(SceneSampler, texCoord - vec2(texel.x, 0.0)).rgb
                + texture(SceneSampler, texCoord + vec2(0.0, texel.y)).rgb
                + texture(SceneSampler, texCoord - vec2(0.0, texel.y)).rgb;
    colour += (colour - around * 0.25) * 0.25;

    vec3 bloom = texture(BloomSampler, texCoord).rgb;
    colour += bloom * 0.35 * Strength.z;

    // The frame is already display-referred; work in linear light for the
    // curve so shadows keep their detail.
    vec3 linear = pow(max(colour, 0.0), vec3(2.2));
    float daylight = SunDirView.w;
    float exposure = Strength.w * mix(1.25, 1.12, daylight);
    linear *= exposure;
    // Blend the filmic curve in rather than replace: the vanilla palette is
    // part of the look, we just want highlights to roll off.
    vec3 mapped = mix(linear, aces(linear * 1.15), 0.7);
    colour = pow(mapped, vec3(1.0 / 2.2));

    // A little more saturation, then vignette.
    float luma = dot(colour, vec3(0.2126, 0.7152, 0.0722));
    colour = mix(vec3(luma), colour, 1.12);
    vec2 centred = texCoord - 0.5;
    colour *= 1.0 - dot(centred, centred) * 0.45;

    fragColor = vec4(clamp(colour, 0.0, 1.0), 1.0);
}
