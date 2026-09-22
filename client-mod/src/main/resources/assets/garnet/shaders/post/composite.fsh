#version 330
#extension GL_ARB_separate_shader_objects : require

// Pass 4: apply the lighting to the frame, shade water surfaces, then add
// atmosphere: aerial perspective that takes on the sun's colour, and light
// shafts where the sky shows through between things.

#include <garnet:frame.glsl>

uniform sampler2D SceneSampler;
uniform sampler2D DepthSampler;
uniform sampler2D LightSampler;
uniform sampler2D TerrainMapSampler;
uniform sampler2D CloudsSampler;

layout(std140) uniform SamplerInfo {
    vec2 OutSize;
    vec2 SceneSize;
    vec2 DepthSize;
    vec2 LightSize;
    vec2 TerrainMapSize;
    vec2 CloudsSize;
};

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

const int SHAFT_SAMPLES = 40;
const int REFLECTION_STEPS = 28;

// How much of the sky is visible along the line from this pixel to the sun.
float lightShafts(vec2 sunUv, float noise) {
    vec2 delta = (sunUv - texCoord) / float(SHAFT_SAMPLES);
    // Don't reach too far across the screen; keeps the effect local.
    delta *= 0.85;
    // Start each pixel at a different point along the line, or the steps
    // show up as stripes across a smooth sky.
    vec2 uv = texCoord + delta * (noise * 2.0 - 0.5);
    float light = 0.0;
    float weight = 1.0;
    float decay = 0.94;
    for (int i = 0; i < SHAFT_SAMPLES; i++) {
        uv += delta;
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) break;
        float d = texture(DepthSampler, uv).r;
        light += (isSky(d) ? 1.0 : 0.0) * weight;
        weight *= decay;
    }
    return light / float(SHAFT_SAMPLES);
}

// Sunlight focused by the ripples into a moving net of bright lines on the
// bottom. Three drifting wave fronts, sharpened; cheap, and close enough to
// the real thing at a glance.
float caustics(vec2 xz, float time) {
    float v = 0.0;
    for (int i = 0; i < 3; i++) {
        float angle = float(i) * 2.0944 + 0.4;
        vec2 dir = vec2(cos(angle), sin(angle));
        v += sin(dot(xz, dir) * 2.3 + time * (1.1 + 0.17 * float(i)));
    }
    float lines = max(0.0, v / 3.0);
    return pow(lines, 4.0);
}

// Air into water, the real thing rather than Schlick: it holds up at the
// grazing angles you get looking across a sea.
float waterFresnel(float cosi) {
    const float n2 = 1.33477;
    float sint = sqrt(max(0.0, 1.0 - cosi * cosi)) / n2;
    if (sint >= 1.0) return 1.0;
    float cost = sqrt(max(0.0, 1.0 - sint * sint));
    float rs = (cosi - cost * n2) / (cosi + cost * n2);
    float rp = (cost - cosi * n2) / (cost + cosi * n2);
    return clamp((rs * rs + rp * rp) * 0.5, 0.02, 1.0);
}

// A GGX highlight for the sun, which spreads out with distance so far
// crests do not flicker from one pixel to the next.
vec3 sunGlint(vec3 n, vec3 viewDir, vec3 sunCol, float dist) {
    vec3 l = normalize(SunDirView.xyz);
    vec3 h = normalize(l - viewDir);
    float ndoth = max(dot(n, h), 0.0);
    float ndotl = max(dot(n, l), 0.0);
    float ndotv = max(dot(n, -viewDir), 0.001);
    float rough = clamp(0.045 + dist * 0.0009, 0.02, 1.0);
    float a2 = rough * rough;
    float denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
    float d = a2 / (3.14159265 * denom * denom);
    float k = (rough + 1.0) * (rough + 1.0) / 8.0;
    float g = (ndotl / (ndotl * (1.0 - k) + k)) * (ndotv / (ndotv * (1.0 - k) + k));
    float f = 0.02 + pow(1.0 - max(dot(h, -viewDir), 0.0), 5.0) * 0.98;
    return sunCol * (d * g * f * ndotl / (4.0 * ndotv));
}

// March a reflected ray through the depth buffer; the hit uv, or -1 for none.
vec2 reflectionHit(vec3 origin, vec3 dir, float noise) {
    vec3 ray = origin;
    vec3 step = dir * 0.3;
    ray += step * noise;
    for (int i = 0; i < REFLECTION_STEPS; i++) {
        ray += step;
        step *= 1.15;
        vec3 s = project(ray);
        if (s.x < 0.0 || s.x > 1.0 || s.y < 0.0 || s.y > 1.0 || s.z < 0.0) return vec2(-1.0);
        float d = texture(DepthSampler, s.xy).r;
        if (isSky(d)) continue;
        vec3 scene = viewPosition(s.xy, d);
        float dz = scene.z - ray.z;
        if (dz > 0.02 && dz < length(step) * 2.5 + 0.3) return s.xy;
    }
    return vec2(-1.0);
}

/**
 * Water, shaded the way a deep-water renderer does it: the light that comes
 * back out of the water is what vanilla drew, dimmed along its path through
 * the water by how strongly water absorbs each colour, plus the light that
 * scattered back before it ever reached the bottom. On top of that goes the
 * reflection of the world, weighted by the Fresnel of the surface, and the
 * sun's own highlight.
 */
vec3 shadeWater(vec3 colour, vec3 p, vec3 worldRel, float sun, float noise, vec3 sunCol) {
    float daylight = SunDirView.w;
    float rain = FogParams.z;
    float dist = length(p);
    float detail = smoothstep(110.0, 18.0, dist);

    // Still water: the surface lies flat, the way vanilla draws it.
    vec3 nWorld = vec3(0.0, 1.0, 0.0);
    vec3 nView = normalize((ViewMat * vec4(nWorld, 0.0)).xyz);
    vec3 viewDir = normalize(p);
    float cosTheta = clamp(dot(-viewDir, nView), 0.0, 1.0);

    // How far light travelled through the water to reach the eye.
    float depthW = mapWaterDepth(TerrainMapSampler, worldRel.xz);
    float path = depthW / max(cosTheta, 0.25);

    // Light bending through the surface gathers into a faint net on the
    // bottom, strongest in the shallows and gone once the water has
    // absorbed it.
    vec3 bottom = colour;
    float net = caustics(worldRel.xz + SunDirWorld.xz * depthW * 0.5, CameraPos.w * 0.8);
    bottom *= 1.0 + net * sun * daylight * (1.0 - rain) * exp(-path * 0.25) * 0.55;

    // Red goes first, then green; what is left of the bottom is blue-green.
    vec3 transmit = exp(-vec3(0.34, 0.07, 0.045) * path);
    vec3 absorbed = vec3(1.0) - transmit;
    vec3 deep = mix(vec3(0.02, 0.10, 0.15), SkyColor.rgb * 0.35, 0.35) * mix(0.15, 1.0, daylight);
    vec3 scatter = (sunCol * 0.5 + SkyColor.rgb * 0.3) * deep * sun * daylight;
    vec3 body = bottom * transmit + deep * absorbed + scatter * absorbed * 0.5;

    // What the surface mirrors: the scene where the ray lands, the sky where
    // it does not.
    vec3 reflectDir = reflect(viewDir, nView);
    vec3 reflectWorld = normalize((ViewInv * vec4(reflectDir, 0.0)).xyz);
    float sunAmount = pow(max(dot(reflectWorld, normalize(SunDirWorld.xyz)), 0.0), 8.0);
    vec3 sky = mix(SkyColor.rgb * 1.05, sunCol, sunAmount * daylight * 0.5) * mix(0.25, 1.0, daylight);
    sky = mix(sky, vec3(0.5, 0.53, 0.58) * mix(0.3, 1.0, daylight), rain * 0.7);
    vec3 reflection = sky;
    vec2 hit = reflectionHit(p + nView * 0.05, reflectDir, noise);
    if (hit.x >= 0.0) {
        vec2 border = smoothstep(0.0, 0.2, hit) * smoothstep(0.0, 0.2, 1.0 - hit);
        float near = smoothstep(90.0, 25.0, dist); // far reflections only smear
        reflection = mix(sky, texture(SceneSampler, hit).rgb, border.x * border.y * near * 0.85);
    }

    float fresnel = waterFresnel(cosTheta);
    vec3 glint = sunGlint(nView, viewDir, sunCol, dist) * sun * daylight * (1.0 - rain);
    vec3 lit = mix(body, reflection, fresnel) + glint;

    // A thin pale edge where the water runs out over the sand.
    float foam = smoothstep(1.4, 0.35, depthW) * mix(0.4, 1.0, detail);
    lit = mix(lit, mix(vec3(0.86, 0.91, 0.93), sunCol, 0.15), foam * 0.18);

    // Thin water at the shore keeps the sand showing through.
    float shore = clamp(depthW / 0.9, 0.0, 1.0);
    return mix(colour, mix(colour, lit, shore), MapParams.w);
}

void main() {
    vec3 colour = texture(SceneSampler, texCoord).rgb;
    float depth = texture(DepthSampler, texCoord).r;
    float daylight = SunDirView.w;
    float rain = FogParams.z;
    bool underwater = FogParams.w > 0.5;
    float noise = interleavedNoise(gl_FragCoord.xy);
    vec3 sunCol = sunColour();

    // The held item sits right in front of the camera; leave it alone.
    bool heldItem = !isSky(depth) && length(viewPosition(texCoord, depth)) < 1.0;

    if (!isSky(depth) && !underwater && !heldItem) {
        vec3 light = texture(LightSampler, texCoord).rgb;
        float ao = light.r;
        float sun = light.g;
        bool water = light.b > 0.5 && MapParams.w > 0.0;

        vec3 p = viewPosition(texCoord, depth);
        vec3 worldRel = worldRelative(p);
        if (water) {
            colour = shadeWater(colour, p, worldRel, sun, noise, sunCol);
            ao = 1.0;
        }

        // Clouds take a little of the sun away from what they pass over.
        sun = min(sun, 1.0 - cloudShadow(CloudsSampler, worldRel) * shadowWeight());

        // Shadowed areas keep the sky's ambient light; lit areas get the sun.
        float shade = mix(1.0, sun, Strength.y * shadowWeight() * (1.0 - rain * 0.8));
        vec3 shadowTint = mix(vec3(0.70, 0.74, 0.84), vec3(1.0), shade);
        colour *= shadowTint;
        // Lit surfaces get a touch of the sun's colour; kept subtle so the
        // vanilla palette stays recognisable.
        vec3 warm = mix(vec3(1.0), normalize(sunCol) * 1.7, 0.06);
        colour *= mix(vec3(1.0), warm, daylight * shade);
        colour *= mix(1.0, ao, 0.9);

        // Aerial perspective: distance haze in the sky's colour, glowing near the sun.
        float dist = length(p);
        vec3 viewDir = normalize(p);
        float sunAmount = pow(max(dot(viewDir, normalize(SunDirView.xyz)), 0.0), 6.0);
        vec3 haze = mix(SkyColor.rgb, sunCol, sunAmount * daylight * 0.6);
        haze = mix(haze, vec3(0.55, 0.58, 0.62), rain * 0.7);
        float density = (0.0010 + rain * 0.0035) * mix(1.3, 1.0, daylight);
        float fog = 1.0 - exp(-dist * density);
        // Against the render distance, which holds still: the environmental
        // fog closes in while chunks load and would make the haze breathe.
        fog *= smoothstep(0.1, 1.0, dist / max(FogParams.y, 1.0)) * 0.85 + 0.15;
        colour = mix(colour, haze, clamp(fog, 0.0, 0.55));
    }

    // Light shafts: only when the sun is on screen, in front of the camera.
    if (Misc.w > 0.0 && daylight > 0.0 && !underwater) {
        vec3 sunView = normalize(SunDirView.xyz) * 500.0;
        vec3 s = project(sunView);
        if (s.x >= -0.3 && s.x <= 1.3 && s.y >= -0.3 && s.y <= 1.3) {
            float shafts = lightShafts(s.xy, whiteNoise(gl_FragCoord.xy));
            vec2 centred = s.xy - 0.5;
            float onScreen = 1.0 - smoothstep(0.55, 0.9, length(centred));
            // Rays read as light in the air between things, so they are
            // weaker where the pixel is open sky to begin with.
            float inAir = isSky(depth) ? 0.45 : 1.0;
            colour += sunCol * shafts * 0.16 * Misc.w * daylight * (1.0 - rain) * onScreen * inAir;
        }
    }

    fragColor = vec4(colour, 1.0);
}
