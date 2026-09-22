// Shadow mapping helpers shared by the shadow pass and the lighting passes.
//
// Iris renders the shadow pass with `shadowModelView` and `shadowProjection`.
// We distort the shadow map so texels near the player get more resolution
// (the classic trick most packs use), which must be applied identically in
// shadow.vsh and when sampling.

const float SHADOW_DISTORT_FACTOR = 0.85;
const float SHADOW_BIAS = 0.0009;

// Bends shadow clip coordinates toward the centre.
vec3 distortShadow(vec3 clip) {
    float len = length(clip.xy);
    float factor = mix(1.0, len, SHADOW_DISTORT_FACTOR);
    return vec3(clip.xy / max(factor, 0.0001), clip.z * 0.2);
}

// Returns how lit a world-space point is: 1 = fully in the sun.
float sampleShadow(sampler2D shadowMap, mat4 shadowModelView, mat4 shadowProjection, vec3 worldPos, vec3 normalWorld, vec3 sunWorld, vec2 fragCoord, float softness) {
    vec4 shadowView = shadowModelView * vec4(worldPos, 1.0);
    vec4 shadowClip = shadowProjection * shadowView;
    vec3 ndc = shadowClip.xyz / shadowClip.w;
    // Slope-scaled bias: surfaces at grazing angles need more.
    float cosTheta = clamp(dot(normalWorld, sunWorld), 0.0, 1.0);
    float bias = SHADOW_BIAS * (1.0 + 4.0 * (1.0 - cosTheta));
    vec3 distorted = distortShadow(ndc);
    vec3 shadowScreen = distorted * 0.5 + 0.5;
    if (shadowScreen.x < 0.0 || shadowScreen.x > 1.0 || shadowScreen.y < 0.0 || shadowScreen.y > 1.0) {
        return 1.0;
    }
    // Rotated Poisson-style disc of 8 taps, dithered per pixel.
    float angle = dither(fragCoord) * 6.2831853;
    float c = cos(angle);
    float s = sin(angle);
    mat2 rot = mat2(c, -s, s, c);
    float radius = softness * 1.5 / 2048.0;
    vec2 taps[8];
    taps[0] = vec2(-0.7071, 0.7071);
    taps[1] = vec2(-0.0, -0.8750);
    taps[2] = vec2(0.5303, 0.5303);
    taps[3] = vec2(-0.6250, -0.0);
    taps[4] = vec2(0.3536, -0.3536);
    taps[5] = vec2(-0.0, 0.3750);
    taps[6] = vec2(-0.1768, -0.1768);
    taps[7] = vec2(0.1250, 0.0);
    float lit = 0.0;
    for (int i = 0; i < 8; i++) {
        vec2 offset = rot * taps[i] * radius;
        float depth = texture2D(shadowMap, shadowScreen.xy + offset).r;
        lit += step(shadowScreen.z - bias, depth);
    }
    return lit / 8.0;
}
