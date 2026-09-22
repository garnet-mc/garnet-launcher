#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 5: keep only the bright parts of the scene for bloom, softened by
// averaging a few neighbours so single bright pixels don't sparkle.

#include <garnet:frame.glsl>

uniform sampler2D SceneSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 SceneSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

vec3 bright(vec2 uv) {
    vec3 c = texture(SceneSampler, uv).rgb;
    float luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    float threshold = 0.72;
    float soft = 0.18;
    float knee = clamp((luma - threshold + soft) / (2.0 * soft), 0.0, 1.0);
    float contribution = max(luma - threshold, knee * knee * soft) / max(luma, 0.0001);
    return c * contribution;
}

void main() {
    vec2 texel = 2.0 / SceneSize;
    vec3 sum = bright(texCoord) * 0.4;
    sum += bright(texCoord + vec2(texel.x, texel.y)) * 0.15;
    sum += bright(texCoord + vec2(-texel.x, texel.y)) * 0.15;
    sum += bright(texCoord + vec2(texel.x, -texel.y)) * 0.15;
    sum += bright(texCoord + vec2(-texel.x, -texel.y)) * 0.15;
    fragColor = vec4(sum, 1.0);
}
