// Options the user can change in the shader settings screen. Iris rewrites
// these defines when a slider or toggle changes.

#define SHADOWS_ENABLED
#define SHADOW_SOFTNESS 1.0   // [0.0 0.5 1.0 1.5 2.0 3.0]
#define AO_ENABLED
#define AO_STRENGTH 1.0       // [0.25 0.5 0.75 1.0 1.25 1.5 2.0]
#define VOLUMETRIC_LIGHT
#define VL_STRENGTH 1.0       // [0.25 0.5 0.75 1.0 1.5 2.0 3.0]
#define WATER_REFLECTIONS
#define WATER_WAVES 1.0       // [0.0 0.5 1.0 1.5 2.0]
#define BLOOM_ENABLED
#define BLOOM_STRENGTH 1.0    // [0.25 0.5 0.75 1.0 1.5 2.0]
//#define RTGI_ENABLED
#define RTGI_SAMPLES 4        // [2 4 8 12 16]
#define WAVING_PLANTS
#define EXPOSURE 1.0          // [0.5 0.75 1.0 1.25 1.5 2.0]

// G-buffer layout (see gbuffers_terrain.fsh):
//   colortex0  albedo rgb, alpha
//   colortex1  world normal (xyz packed 0..1), block light
//   colortex2  sky light, material id / 10, ao from vertex colour
//   colortex3  lit scene (HDR), carried through the composite passes
//   colortex4  bloom
