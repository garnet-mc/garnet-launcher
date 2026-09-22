// Shared helpers: noise, encoding, coordinate transforms, the sky model.

// ---------- noise ----------

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float hash13(vec3 p3) {
    p3 = fract(p3 * 0.1031);
    p3 += dot(p3, p3.zyx + 31.32);
    return fract((p3.x + p3.y) * p3.z);
}

// Interleaved gradient noise: a good per-pixel dither for sample offsets.
float dither(vec2 fragCoord) {
    return fract(52.9829189 * fract(0.06711056 * fragCoord.x + 0.00583715 * fragCoord.y));
}

float noise2(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash12(i);
    float b = hash12(i + vec2(1.0, 0.0));
    float c = hash12(i + vec2(0.0, 1.0));
    float d = hash12(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm2(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) {
        v += a * noise2(p);
        p = p * 2.03 + vec2(17.3, 9.1);
        a *= 0.5;
    }
    return v;
}

// ---------- packing ----------

vec3 encodeNormal(vec3 n) {
    return n * 0.5 + 0.5;
}

vec3 decodeNormal(vec3 e) {
    return normalize(e * 2.0 - 1.0);
}

// ---------- space transforms ----------

// Screen (0..1, depth) -> view space.
vec3 screenToView(vec3 screen, mat4 projectionInverse) {
    vec4 clip = vec4(screen * 2.0 - 1.0, 1.0);
    vec4 view = projectionInverse * clip;
    return view.xyz / view.w;
}

// View space -> screen (0..1).
vec3 viewToScreen(vec3 view, mat4 projection) {
    vec4 clip = projection * vec4(view, 1.0);
    return (clip.xyz / clip.w) * 0.5 + 0.5;
}

float linearDepth(float depth, float near, float far) {
    return (2.0 * near) / (far + near - depth * (far - near));
}

// ---------- light and sky ----------

// Where the sun is in the day: 1 at noon, 0 at night, soft edges at dusk.
float dayFactor(vec3 sunDirView, mat4 modelViewInverse) {
    vec3 sunWorld = normalize(mat3(modelViewInverse) * sunDirView);
    return smoothstep(-0.12, 0.18, sunWorld.y);
}

vec3 sunColour(float sunHeight) {
    vec3 noon = vec3(1.0, 0.97, 0.92);
    vec3 dusk = vec3(1.0, 0.55, 0.25);
    float t = smoothstep(0.0, 0.35, sunHeight);
    return mix(dusk, noon, t);
}

vec3 moonColour() {
    return vec3(0.35, 0.45, 0.7);
}

// A compact sky gradient with a warm horizon at dusk and a sun glow.
vec3 skyColour(vec3 dirWorld, vec3 sunWorld, float rain) {
    float sunHeight = sunWorld.y;
    float day = smoothstep(-0.15, 0.2, sunHeight);
    float horizon = 1.0 - clamp(dirWorld.y, 0.0, 1.0);
    horizon = pow(horizon, 3.0);

    vec3 zenithDay = vec3(0.16, 0.36, 0.78);
    vec3 horizonDay = vec3(0.62, 0.76, 0.92);
    vec3 zenithNight = vec3(0.01, 0.015, 0.035);
    vec3 horizonNight = vec3(0.03, 0.04, 0.07);
    vec3 duskTint = vec3(1.0, 0.45, 0.2);

    vec3 zenith = mix(zenithNight, zenithDay, day);
    vec3 hor = mix(horizonNight, horizonDay, day);
    float duskAmount = (1.0 - smoothstep(0.0, 0.3, abs(sunHeight))) * max(dot(normalize(dirWorld.xz), normalize(sunWorld.xz + 0.001)), 0.0);
    hor = mix(hor, duskTint, duskAmount * 0.8);

    vec3 sky = mix(zenith, hor, horizon);

    // Sun glow.
    float sunDot = max(dot(dirWorld, sunWorld), 0.0);
    sky += sunColour(sunHeight) * pow(sunDot, 64.0) * 0.6 * day;
    sky += sunColour(sunHeight) * pow(sunDot, 8.0) * 0.12 * day;

    // Rain greys everything out.
    vec3 overcast = vec3(0.35, 0.37, 0.4) * mix(0.15, 1.0, day);
    sky = mix(sky, overcast, rain * 0.85);
    return sky;
}

// ---------- fog ----------

vec3 applyFog(vec3 colour, float distance, vec3 fogTint, float density, float startFraction, float farPlane) {
    float f = clamp((distance - farPlane * startFraction) / (farPlane * (1.0 - startFraction)), 0.0, 1.0);
    f = 1.0 - exp(-f * f * density * 4.0);
    return mix(colour, fogTint, f);
}
