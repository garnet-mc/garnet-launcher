#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 1: from the depth buffer and the terrain map, work out how much light
// each pixel gets. Red = ambient occlusion (1 = open), green = sun
// visibility (1 = lit), blue = 1 on a water surface. Red and green are noisy
// here and smoothed by the blur passes.

#include <garnet:frame.glsl>

uniform sampler2D DepthSampler;
uniform sampler2D TerrainMapSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 DepthSize;
    vec2 TerrainMapSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

const int AO_SAMPLES = 8;
const int SHADOW_STEPS = 12;
const int TERRAIN_STEPS = 22;

// Surface normal from neighbouring depths, picking the smaller difference on
// each axis so edges don't smear.
vec3 normalFromDepth(vec2 uv, vec3 p) {
    vec2 texel = 1.0 / DepthSize;
    vec3 px1 = viewPosition(uv + vec2(texel.x, 0.0), texture(DepthSampler, uv + vec2(texel.x, 0.0)).r);
    vec3 px2 = viewPosition(uv - vec2(texel.x, 0.0), texture(DepthSampler, uv - vec2(texel.x, 0.0)).r);
    vec3 py1 = viewPosition(uv + vec2(0.0, texel.y), texture(DepthSampler, uv + vec2(0.0, texel.y)).r);
    vec3 py2 = viewPosition(uv - vec2(0.0, texel.y), texture(DepthSampler, uv - vec2(0.0, texel.y)).r);
    vec3 dx = length(px1 - p) < length(p - px2) ? px1 - p : p - px2;
    vec3 dy = length(py1 - p) < length(p - py2) ? py1 - p : p - py2;
    return normalize(cross(dx, dy));
}

float ambientOcclusion(vec2 uv, vec3 p, vec3 n, float noise) {
    float radius = 1.4;
    float occlusion = 0.0;
    // Golden-angle spiral in the tangent plane, rotated by noise per pixel.
    for (int i = 0; i < AO_SAMPLES; i++) {
        float f = (float(i) + 0.5) / float(AO_SAMPLES);
        float angle = f * 6.2831853 * 3.0 + noise * 6.2831853;
        float r = sqrt(f) * radius;
        // Random direction in the hemisphere around n.
        vec3 dir = normalize(vec3(cos(angle), sin(angle), 0.6 + 0.4 * f));
        vec3 t = normalize(abs(n.y) < 0.99 ? cross(n, vec3(0.0, 1.0, 0.0)) : cross(n, vec3(1.0, 0.0, 0.0)));
        vec3 b = cross(n, t);
        vec3 samplePos = p + (t * dir.x + b * dir.y + n * dir.z) * r;
        vec3 s = project(samplePos);
        if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0) continue;
        float sceneDepth = texture(DepthSampler, s.xy).r;
        vec3 scenePos = viewPosition(s.xy, sceneDepth);
        float dz = scenePos.z - samplePos.z; // > 0: scene surface is in front of the sample
        float rangeCheck = smoothstep(0.0, 1.0, radius / max(abs(p.z - scenePos.z), 0.001));
        occlusion += (dz > 0.05 ? 1.0 : 0.0) * rangeCheck;
    }
    return 1.0 - (occlusion / float(AO_SAMPLES)) * Strength.x;
}

// Contact shadows: march from the surface towards the sun through the depth
// buffer. Anything the ray passes behind casts a shadow on this pixel.
float sunVisibility(vec2 uv, vec3 p, vec3 n, float noise) {
    if (SunDirView.w <= 0.001 || Strength.y <= 0.0) return 1.0;
    vec3 sun = normalize(SunDirView.xyz);
    float ndotl = dot(n, sun);
    // Faces turned away from the sun are in shadow, but vanilla already
    // shades block sides, so don't black them out completely.
    if (ndotl <= 0.0) return 0.45;
    float maxDistance = 8.0;
    vec3 start = p + n * 0.08 + sun * 0.05;
    float visibility = 1.0;
    float stepLength = maxDistance / float(SHADOW_STEPS);
    for (int i = 0; i < SHADOW_STEPS; i++) {
        float t = (float(i) + noise) * stepLength;
        vec3 samplePos = start + sun * t;
        vec3 s = project(samplePos);
        if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0) break;
        float sceneDepth = texture(DepthSampler, s.xy).r;
        if (isSky(sceneDepth)) continue;
        vec3 scenePos = viewPosition(s.xy, sceneDepth);
        float dz = scenePos.z - samplePos.z;
        float thickness = 0.6 + t * 0.15;
        if (dz > 0.02 && dz < thickness) {
            // Soft edge: partial credit near the end of the ray.
            visibility = min(visibility, 0.15 + 0.85 * smoothstep(maxDistance * 0.6, maxDistance, t));
        }
    }
    // Grazing light is a little weaker; kept gentle because vanilla's own
    // face shading is still underneath.
    float lambert = mix(0.8, 1.0, clamp(ndotl, 0.0, 1.0));
    return mix(0.45, 1.0, visibility * lambert);
}

// Long shadows: march from the surface towards the sun over the terrain map.
// Passing close over a ridge darkens a little (a cheap penumbra); going
// under it is full shadow.
float terrainShadow(vec3 worldRel, vec3 nWorld, float noise) {
    if (MapParams.z < 0.5 || SunDirWorld.w <= 0.0 || Strength.y <= 0.0) return 1.0;
    vec3 sun = normalize(SunDirWorld.xyz);
    vec3 pos = worldRel + nWorld * 0.05;
    float result = 1.0;
    float t = 0.9;
    for (int i = 0; i < TERRAIN_STEPS; i++) {
        vec3 q = pos + sun * t;
        if (!insideMap(q.xz) || q.y > 330.0 || t > 48.0) break;
        float gap = q.y - mapSurface(TerrainMapSampler, q.xz);
        if (gap < -0.05) return 0.0;
        // A narrow penumbra: only rays that just skim a ridge get darker.
        result = min(result, 12.0 * gap / t);
        t = t * 1.16 + 0.4;
    }
    return clamp(result, 0.0, 1.0);
}

// 1 when this pixel sits on the top of a body of water.
float waterSurface(vec3 worldRel, vec3 nWorld, float dist) {
    if (MapParams.z < 0.5 || !insideMap(worldRel.xz)) return 0.0;
    if (mapWaterDepth(TerrainMapSampler, worldRel.xz) <= 0.0) return 0.0;
    float top = mapSurface(TerrainMapSampler, worldRel.xz) - 0.11; // water sits 8/9 up its block
    float tolerance = 0.2 + dist * 0.012;
    return (abs(worldRel.y - top) < tolerance && nWorld.y > 0.6) ? 1.0 : 0.0;
}

void main() {
    float depth = texture(DepthSampler, texCoord).r;
    if (isSky(depth)) {
        fragColor = vec4(1.0, 1.0, 0.0, 1.0);
        return;
    }
    vec3 p = viewPosition(texCoord, depth);
    vec3 n = normalFromDepth(texCoord, p);
    float noise = interleavedNoise(gl_FragCoord.xy);
    vec3 worldRel = worldRelative(p);
    vec3 nWorld = normalize((ViewInv * vec4(n, 0.0)).xyz);
    float dist = length(p);

    float ao = ambientOcclusion(texCoord, p, n, noise);
    float sun = sunVisibility(texCoord, p, n, noise);
    float terrain = terrainShadow(worldRel, nWorld, noise);
    float water = waterSurface(worldRel, nWorld, dist);

    // Fade the screen-space work out in the distance where depth precision
    // gets poor, and right in front of the camera where the held item is
    // drawn. The terrain shadow is stable at any distance.
    float near = smoothstep(0.35, 0.9, dist);
    float fade = smoothstep(120.0, 60.0, dist) * near;
    ao = mix(1.0, ao, fade);
    sun = mix(1.0, sun, fade);
    sun = min(sun, mix(1.0, terrain, near));

    fragColor = vec4(ao, sun, water, 1.0);
}
