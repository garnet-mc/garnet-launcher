package org.garnetmc.client.render;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;
import net.minecraft.tags.FluidTags;
import net.minecraft.util.Mth;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import org.garnetmc.client.GarnetClient;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * A top-down map of the terrain around the player, kept on the GPU as a
 * 512x512 texture and read by the render passes.
 *
 * Per block column: R,G = height of the top surface (16 bit, +64 so it is
 * never negative), B = depth of water at that column (0 = no water). The
 * shaders ray-march this map towards the sun for long, soft shadows from
 * hills, trees and buildings, and use the water depth to find and shade
 * water surfaces, all without rendering the world a second time.
 *
 * Rebuilt on a worker thread every half second from the chunks the client
 * already has; uploaded on the render thread just before our passes run.
 */
public final class TerrainMap {
    public static final Identifier ID = Identifier.fromNamespaceAndPath("garnet", "terrain_map");
    public static final int SIZE = 512;
    private static final int REBUILD_EVERY_TICKS = 10;
    private static final int MAX_WATER_DEPTH = 64;

    private static final ExecutorService worker = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "garnet-terrain-map");
        t.setDaemon(true);
        return t;
    });

    private static DynamicTexture texture;
    private static volatile int[] pending;
    private static volatile int pendingOriginX;
    private static volatile int pendingOriginZ;
    private static volatile boolean building;
    private static int ticks;

    // What the texture on the GPU currently covers.
    private static int originX;
    private static int originZ;
    private static boolean ready;

    private TerrainMap() {}

    public static void register(Minecraft mc) {
        texture = new DynamicTexture(() -> "Garnet terrain map", SIZE, SIZE, false);
        mc.getTextureManager().register(ID, texture);
    }

    public static int originX() {
        return originX;
    }

    public static int originZ() {
        return originZ;
    }

    public static boolean isReady() {
        return ready;
    }

    /** Binds the map to a pass that declares a `TerrainMap` sampler. */
    public static void bind(com.mojang.renderpearl.api.commands.RenderPass pass) {
        if (texture == null || !ready) return;
        com.mojang.renderpearl.api.textures.GpuTextureView view = texture.getTextureView();
        if (view == null) return;
        com.mojang.renderpearl.api.textures.GpuSampler sampler = com.mojang.blaze3d.systems.RenderSystem.getSamplerCache()
                .getClampToEdge(com.mojang.renderpearl.api.textures.FilterMode.NEAREST);
        pass.setUniform("TerrainMapSampler", view, sampler);
    }

    /** Client tick: kick off a rebuild now and then. */
    public static void tick(Minecraft mc) {
        if (mc.level == null || mc.player == null) {
            ready = false;
            return;
        }
        if (++ticks % REBUILD_EVERY_TICKS != 0 || building) return;
        building = true;
        ClientLevel level = mc.level;
        int ox = (Mth.floor(mc.player.getX()) - SIZE / 2) & ~15;
        int oz = (Mth.floor(mc.player.getZ()) - SIZE / 2) & ~15;
        worker.execute(() -> {
            try {
                int[] pixels = build(level, ox, oz);
                pendingOriginX = ox;
                pendingOriginZ = oz;
                pending = pixels;
            } catch (Exception e) {
                GarnetClient.LOG.debug("terrain map build failed", e);
            } finally {
                building = false;
            }
        });
    }

    /** Render thread, with no render pass open: push the latest map to the GPU. */
    public static void upload() {
        int[] pixels = pending;
        if (pixels == null || texture == null) return;
        pending = null;
        NativeImage image = texture.getPixels();
        if (image == null) return;
        for (int y = 0; y < SIZE; y++) {
            int row = y * SIZE;
            for (int x = 0; x < SIZE; x++) {
                image.setPixelABGR(x, y, pixels[row + x]);
            }
        }
        texture.upload();
        originX = pendingOriginX;
        originZ = pendingOriginZ;
        ready = true;
    }

    private static int[] build(ClientLevel level, int ox, int oz) {
        int[] pixels = new int[SIZE * SIZE];
        int chunks = SIZE / 16;
        int minY = level.getMinY();
        for (int cz = 0; cz < chunks; cz++) {
            for (int cx = 0; cx < chunks; cx++) {
                LevelChunk chunk = level.getChunkSource().getChunk((ox >> 4) + cx, (oz >> 4) + cz, ChunkStatus.FULL, false);
                if (chunk == null) continue;
                for (int z = 0; z < 16; z++) {
                    for (int x = 0; x < 16; x++) {
                        int top = chunk.getHeight(Heightmap.Types.MOTION_BLOCKING, x, z);
                        int surface = top + 1; // plane on top of the highest block
                        int water = 0;
                        if (top >= minY && chunk.getFluidState(x, top, z).is(FluidTags.WATER)) {
                            int y = top;
                            while (y > minY && water < MAX_WATER_DEPTH && chunk.getFluidState(x, y, z).is(FluidTags.WATER)) {
                                water++;
                                y--;
                            }
                        }
                        int encoded = Math.max(0, Math.min(65535, surface + 64));
                        int r = encoded & 0xFF;
                        int g = encoded >> 8;
                        int b = Math.min(255, water);
                        pixels[(cz * 16 + z) * SIZE + cx * 16 + x] = (0xFF << 24) | (b << 16) | (g << 8) | r;
                    }
                }
            }
        }
        return pixels;
    }
}
