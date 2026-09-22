# Garnet Shaders

A shader pack for [Iris](https://irisshaders.dev) (and OptiFine-format loaders), made for Garnet but usable anywhere.

- Soft, dithered shadows with a distorted shadow map for detail near the player
- Screen-space ambient occlusion
- Procedural sky with sun, moon, stars, dusk colours and rain
- Water with animated waves, Fresnel, screen-space reflections and sun glints
- Volumetric light shafts
- Waving grass, crops and leaves
- Bloom, eye adaptation, ACES tone mapping
- Experimental screen-space ray-traced global illumination (off by default; expensive)

Install: copy this folder into `.minecraft/shaderpacks/` (or let the Garnet launcher do it) and pick it in Options → Video Settings → Shader Packs. Every effect has a switch or slider in the shader settings screen.

The pack is written against the classic `#version 120` program layout, which Iris supports on every current Minecraft version. It has not yet been tuned on a wide range of GPUs; if something renders wrong, please open an issue with your GPU and a screenshot.
