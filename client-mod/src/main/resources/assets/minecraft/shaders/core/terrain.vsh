#version 330
#extension GL_ARB_separate_shader_objects : require

// Garnet: vanilla's terrain shader with wind added.
//
// Grass, crops and leaves arrive here marked by Wind.markQuad: their vertex
// colour has an alpha of 254 down to 248 instead of 255, and how deep that
// dent is says how far the vertex may sway. Everything else keeps alpha 255
// and is drawn exactly as vanilla would.

#include <minecraft:fog.glsl>
#include <minecraft:globals.glsl>
#include <minecraft:projection.glsl>
#include <minecraft:sample_lightmap.glsl>
#include <minecraft:terrainglobals.glsl>
#ifndef MULTIDRAW_TERRAIN
    #include <minecraft:chunksection.glsl>
#endif

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
layout(location = 3) in ivec2 UV2;
#ifdef MULTIDRAW_TERRAIN
layout(location = 4) in ivec3 ChunkPosition;
layout(location = 5) in float ChunkVisibility;
#endif

#ifndef OIT_ALPHA_ONLY
uniform sampler2D Sampler2;
#endif

layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
layout(location = 2) out vec4 vertexColor;
layout(location = 3) out vec2 texCoord0;
layout(location = 4) out float chunkVisibility;

// GameTime runs 0..1 over a Minecraft day and then wraps. Both wind rates are
// a whole number of cycles per day, so the breeze carries on across midnight
// instead of jumping.
const float WIND_SLOW = 6.2831853 * 535.0;
const float WIND_FAST = 6.2831853 * 909.0;
const vec2 WIND_DIRECTION = vec2(0.80, 0.60);
const float WIND_STEP = 0.035; // blocks of sway per level of the mark

// How far this vertex may move, from the dent the mod put in the alpha.
float windReach(float alpha) {
    float level = round((1.0 - alpha) * 255.0);
    return level > 7.5 ? 0.0 : level * WIND_STEP;
}

void main() {
    vec3 pos = Position + (ChunkPosition - CameraBlockPos) + CameraOffset;

    float reach = windReach(Color.a);
    if (reach > 0.0) {
        // Where the block sits in the world sets its phase, so neighbouring
        // plants are never in step. Wrapped every 512 blocks on a whole
        // number of waves, which keeps both the seam and the maths small.
        vec2 grid = mod(Position.xz + vec2(ChunkPosition.xz), 512.0);
        float phase = grid.x * 0.5522 + grid.y * 0.4172;
        float sway = sin(WIND_SLOW * GameTime + phase) * 0.7
                   + sin(WIND_FAST * GameTime + phase * 1.31) * 0.3;
        pos.xz += WIND_DIRECTION * sway * reach;
        pos.y += sin(WIND_FAST * GameTime + phase * 0.7) * reach * 0.15;
    }

    gl_Position = ProjMat * ModelViewMat * vec4(pos, 1.0);

    sphericalVertexDistance = fog_spherical_distance(pos);
    cylindricalVertexDistance = fog_cylindrical_distance(pos);
    #ifndef OIT_ALPHA_ONLY
    vertexColor = Color * sample_lightmap(Sampler2, UV2);
    #else
    vertexColor = Color;
    #endif
    // The mark is only there to be read here; hand the fragment shader the
    // opaque vertex vanilla meant to draw.
    if (reach > 0.0) {
        vertexColor.a = 1.0;
    }
    texCoord0 = UV0;

    const float chunkFullyVisibleRange = 16.0;
    float dist = length(pos);
    chunkVisibility = mix(1.0, ChunkVisibility, clamp((dist - chunkFullyVisibleRange) / chunkFullyVisibleRange, 0.0, 1.0));
}
