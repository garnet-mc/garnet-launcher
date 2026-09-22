#version 330
#extension GL_ARB_separate_shader_objects : require

// Passes 2 and 3: smooth the noisy lighting buffer along one axis without
// bleeding across depth edges (a bilateral blur).

#include <garnet:frame.glsl>

uniform sampler2D LightSampler;
uniform sampler2D DepthSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 LightSize;
    vec2 DepthSize;
};

layout(std140) uniform BlurConfig {
    vec2 Direction;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

const int RADIUS = 6;

void main() {
    float centreDepth = texture(DepthSampler, texCoord).r;
    if (isSky(centreDepth)) {
        fragColor = texture(LightSampler, texCoord);
        return;
    }
    float centreZ = viewPosition(texCoord, centreDepth).z;
    vec2 texel = Direction / LightSize;

    vec4 total = vec4(0.0);
    float weightSum = 0.0;
    for (int i = -RADIUS; i <= RADIUS; i++) {
        vec2 uv = texCoord + texel * float(i);
        float d = texture(DepthSampler, uv).r;
        if (isSky(d)) continue;
        float z = viewPosition(uv, d).z;
        float spatial = exp(-float(i * i) / 12.0);
        float depthWeight = exp(-abs(z - centreZ) * 2.0);
        float w = spatial * depthWeight;
        total += texture(LightSampler, uv) * w;
        weightSum += w;
    }
    fragColor = weightSum > 0.0 ? total / weightSum : texture(LightSampler, texCoord);
}
