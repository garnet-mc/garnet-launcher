# Garnet Shaders

> **Status:** compiles under Iris 1.11 on Minecraft 26.3, but the look is not tuned yet (sky and tint are off). Garnet's primary pipeline is **Garnet Render** inside the client mod, which needs no Iris; this pack is kept for Iris users and will be tuned later.


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
