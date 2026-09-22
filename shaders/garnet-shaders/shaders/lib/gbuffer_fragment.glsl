// Opaque terrain: writes albedo, normal and light data for the deferred pass.

#include "/lib/settings.glsl"
#include "/lib/common.glsl"

uniform sampler2D texture;

varying vec2 texcoord;
varying vec2 lmcoord;
varying vec4 vcolor;
varying vec3 normalWorld;
varying float materialId;

/* DRAWBUFFERS:012 */

void main() {
    vec4 albedo = texture2D(texture, texcoord) * vcolor;
    if (albedo.a < 0.1) discard;

    // Material ids from block.properties are large numbers; store a small
    // class instead: 0 default, 1 foliage, 2 water, 3 emissive.
    float material = 0.0;
    if (materialId == 10000.0 || materialId == 10001.0 || materialId == 10004.0) material = 1.0;
    if (materialId == 10002.0) material = 2.0;
    if (materialId == 10003.0) material = 3.0;

    gl_FragData[0] = vec4(albedo.rgb, 1.0);
    gl_FragData[1] = vec4(encodeNormal(normalWorld), lmcoord.x);
    gl_FragData[2] = vec4(lmcoord.y, material / 10.0, 1.0, 1.0);
}
