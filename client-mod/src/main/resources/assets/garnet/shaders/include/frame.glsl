#ifndef GARNET_FRAME_GLSL
#define GARNET_FRAME_GLSL

// Per-frame data written by GarnetRender.bindFrame. Keep in sync with it.
layout(std140) uniform GarnetFrame {
    mat4 ProjMat;
    mat4 ProjInv;
    mat4 ViewMat;      // world -> view rotation (camera at the origin)
    mat4 ViewInv;
    vec4 SunDirView;   // xyz sun direction in view space, w daylight 0..1
    vec4 SunDirWorld;  // xyz sun direction in world space, w sun height -1..1
    vec4 CameraPos;    // xyz camera position (wrapped), w seconds
    vec4 FogParams;    // x environmental fog end, y render distance end, z rain, w underwater
    vec4 SkyColor;     // rgb sky colour, a sunrise/sunset amount
    vec4 Strength;     // x ambient occlusion, y shadows, z bloom, w exposure
    vec4 Misc;         // x near, y far, z depth is 0..1 (else -1..1), w light shafts
};

// View-space position of the pixel at uv with the given depth-buffer value.
vec3 viewPosition(vec2 uv, float depth) {
    float z = Misc.z > 0.5 ? depth : depth * 2.0 - 1.0;
    vec4 clip = vec4(uv * 2.0 - 1.0, z, 1.0);
    vec4 view = ProjInv * clip;
    return view.xyz / view.w;
}

// Screen uv (0..1) and depth of a view-space point; z < 0 means "behind the camera".
vec3 project(vec3 viewPos) {
    vec4 clip = ProjMat * vec4(viewPos, 1.0);
    if (clip.w <= 0.0) return vec3(-1.0);
    vec3 ndc = clip.xyz / clip.w;
    float depth = Misc.z > 0.5 ? ndc.z : ndc.z * 0.5 + 0.5;
    return vec3(ndc.xy * 0.5 + 0.5, depth);
}

bool isSky(float depth) {
    return depth >= 0.999999;
}

// Cheap per-pixel noise that changes every frame; good enough to hide banding.
float interleavedNoise(vec2 pixel) {
    float t = fract(CameraPos.w * 7.0) * 5.588238;
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y + t));
}

vec3 sunColour() {
    float h = clamp(SunDirWorld.w, -0.2, 1.0);
    vec3 noon = vec3(1.0, 0.96, 0.9);
    vec3 low = vec3(1.0, 0.55, 0.25);
    return mix(low, noon, smoothstep(0.0, 0.35, h));
}

#endif
