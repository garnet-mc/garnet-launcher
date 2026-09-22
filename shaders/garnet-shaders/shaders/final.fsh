#version 120
// The last step: bloom, eye adaptation, ACES tone mapping, gamma, and a
// touch of vignette.

#include "/lib/settings.glsl"

uniform sampler2D colortex3;
uniform sampler2D colortex4;
uniform float viewWidth;
uniform float viewHeight;
uniform ivec2 eyeBrightnessSmooth;
uniform float rainStrength;

varying vec2 texcoord;

// Narkowicz's ACES fit: film-like highlights that roll off instead of clip.
vec3 aces(vec3 x) {
    const float a = 2.51;
    const float b = 0.03;
    const float c = 2.43;
    const float d = 0.59;
    const float e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec3 colour = texture2D(colortex3, texcoord).rgb;

#ifdef BLOOM_ENABLED
    // Sample the half-res bloom with a small blur so it stays smooth.
    vec2 texel = 2.0 / vec2(viewWidth, viewHeight);
    vec3 bloom = vec3(0.0);
    bloom += texture2D(colortex4, texcoord).rgb * 0.4;
    bloom += texture2D(colortex4, texcoord + vec2(texel.x, 0.0)).rgb * 0.15;
    bloom += texture2D(colortex4, texcoord - vec2(texel.x, 0.0)).rgb * 0.15;
    bloom += texture2D(colortex4, texcoord + vec2(0.0, texel.y)).rgb * 0.15;
    bloom += texture2D(colortex4, texcoord - vec2(0.0, texel.y)).rgb * 0.15;
    colour += bloom * 0.22 * BLOOM_STRENGTH;
#endif

    // Eye adaptation: the game reports how bright the player's surroundings
    // are; dark places get a little more exposure, bright ones a little less.
    float eyeSky = float(eyeBrightnessSmooth.y) / 240.0;
    float eyeBlock = float(eyeBrightnessSmooth.x) / 240.0;
    float ambientLevel = max(eyeSky, eyeBlock * 0.6);
    float exposure = EXPOSURE * mix(1.35, 0.85, ambientLevel);
    colour *= exposure;

    colour = aces(colour);
    colour = pow(colour, vec3(1.0 / 2.2));

    // Vignette.
    vec2 centred = texcoord - 0.5;
    float vignette = 1.0 - dot(centred, centred) * 0.35;
    colour *= vignette;

    gl_FragColor = vec4(colour, 1.0);
}
