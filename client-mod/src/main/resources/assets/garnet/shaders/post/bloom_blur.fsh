#version 330
#extension GL_ARB_separate_shader_objects : require

// Passes 6 and 7: a wide gaussian blur of the bright buffer, one axis each.
// Samples land between texels so bilinear filtering doubles the reach.

uniform sampler2D BloomSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 BloomSize;
};

layout(std140) uniform BlurConfig {
    vec2 Direction;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

void main() {
    vec2 step = Direction / BloomSize * 3.0;
    vec3 sum = texture(BloomSampler, texCoord).rgb * 0.227;
    float weights[4] = float[](0.194, 0.121, 0.054, 0.016);
    for (int i = 1; i <= 4; i++) {
        vec2 offset = step * (float(i) * 2.0 - 0.5);
        sum += texture(BloomSampler, texCoord + offset).rgb * weights[i - 1];
        sum += texture(BloomSampler, texCoord - offset).rgb * weights[i - 1];
    }
    fragColor = vec4(sum, 1.0);
}
