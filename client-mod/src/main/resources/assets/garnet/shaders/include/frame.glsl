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
    vec4 CameraPos;    // xyz camera position relative to the terrain map corner (y absolute), w seconds
    vec4 FogParams;    // x environmental fog end, y render distance end, z rain, w underwater
    vec4 SkyColor;     // rgb sky colour, a sunrise/sunset amount
    vec4 Strength;     // x ambient occlusion, y shadows, z bloom, w exposure
    vec4 Misc;         // x near, y far, z depth is 0..1 (else -1..1), w light shafts
    vec4 MapParams;    // x map size in blocks, y 1/size, z map ready, w water strength
    vec4 CloudInfo;    // xy cloud grid shift at the map corner, z cloud height, w strength
    vec4 MapWrap;      // xy where the map's wrap falls, zw unused
};

// World position relative to the terrain map corner (y is the real height).
vec3 worldRelative(vec3 viewPos) {
    return CameraPos.xyz + (ViewInv * vec4(viewPos, 0.0)).xyz;
}

// The texel a map-relative x/z lands in.
vec2 mapCoord(vec2 xz) {
    return fract((xz + MapWrap.xy) * MapParams.y);
}

// Height of the top surface plane at a map-relative x/z.
float mapSurface(sampler2D map, vec2 xz) {
    vec4 t = texture(map, mapCoord(xz));
    return t.r * 255.0 + t.g * 255.0 * 256.0 - 64.0;
}

// Blocks of water under the surface at that column, 0 when there is none.
float mapWaterDepth(sampler2D map, vec2 xz) {
    return texture(map, mapCoord(xz)).b * 255.0;
}

bool insideMap(vec2 xz) {
    return xz.x > 1.0 && xz.y > 1.0 && xz.x < MapParams.x - 1.0 && xz.y < MapParams.x - 1.0;
}

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

// The depth-buffer value of the far plane, i.e. what is left where nothing
// was drawn. Vanilla uses a reversed, infinite projection (1 at the camera,
// 0 at infinity), but this is derived from the matrix so either way works.
float farDepth() {
    float ndc = ProjMat[2][2] / ProjMat[2][3];
    return Misc.z > 0.5 ? ndc : ndc * 0.5 + 0.5;
}

bool isSky(float depth) {
    return abs(depth - farDepth()) < 1e-6;
}

// A fixed per-pixel dither pattern. It must not change between frames or
// the shadows shimmer; the blur passes turn the pattern into smooth shade.
float interleavedNoise(vec2 pixel) {
    return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
}

// Plain white noise, for places where the ordered pattern above would show
// up as stripes rather than grain.
float whiteNoise(vec2 pixel) {
    return fract(sin(dot(pixel, vec2(12.9898, 78.233))) * 43758.5453);
}

// Shadows only carry weight when the sun is well above the horizon; a low
// sun would drape the whole world in shadow.
float shadowWeight() {
    return smoothstep(0.06, 0.40, SunDirWorld.w);
}

// How much of the sun this point loses to the clouds drifting over it: the
// ray to the sun is followed up to the cloud deck and the cloud texture is
// read where it gets there, so the shadow lands under the cloud that casts
// it. 0 when the sky is clear here, the setting's strength when it is not.
float cloudShadow(sampler2D clouds, vec3 worldRel) {
    if (CloudInfo.w <= 0.0 || SunDirWorld.y <= 0.05) return 0.0;
    float travel = (CloudInfo.z - worldRel.y) / SunDirWorld.y;
    if (travel <= 0.0) return 0.0; // above the clouds: nothing left to shade
    vec2 at = worldRel.xz + SunDirWorld.xz * travel + CloudInfo.xy;
    // Clouds are 12 blocks to a cell and the sheet repeats; wrap it here
    // because the sampler holds its edge instead of tiling.
    vec2 uv = fract(at / (12.0 * vec2(textureSize(clouds, 0))));
    return texture(clouds, uv).a * CloudInfo.w;
}

vec3 sunColour() {
    float h = clamp(SunDirWorld.w, -0.2, 1.0);
    vec3 noon = vec3(1.0, 0.96, 0.9);
    vec3 low = vec3(1.0, 0.55, 0.25);
    return mix(low, noon, smoothstep(0.0, 0.35, h));
}

#endif
