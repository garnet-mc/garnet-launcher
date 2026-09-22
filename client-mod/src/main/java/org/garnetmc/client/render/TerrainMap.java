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

import java.util.Arrays;

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
 * The map wraps, so a block column always lands in the same texel wherever
 * the player stands. Walking then costs nothing but filling in the strip of
 * chunks that just came into range, and a few chunks are looked at again
 * every tick so that what people build shows up in the shade.
 */
public final class TerrainMap {
    public static final Identifier ID = Identifier.fromNamespaceAndPath("garnet", "terrain_map");
    public static final int SIZE = 512;
    private static final int CHUNKS = SIZE / 16;
    /** Chunks filled per tick. Sprinting crosses one about every 6 ticks. */
    private static final int FILL_BUDGET = 6;
    /** Chunks looked at again per tick, to catch blocks placed and broken. */
    private static final int SWEEP_BUDGET = 2;
    private static final int UPLOAD_EVERY_TICKS = 4;
    private static final int MAX_WATER_DEPTH = 64;
    private static final long NOTHING = Long.MIN_VALUE;

    private static DynamicTexture texture;

    // Which chunk each texel block currently holds, and which ones want
    // filling again.
    private static final long[] held = new long[CHUNKS * CHUNKS];
    private static final boolean[] stale = new boolean[CHUNKS * CHUNKS];
    private static int cursor;
    private static int sweep;

    // The part of the world the map covers, and whether the GPU has it.
    private static int originX;
    private static int originZ;
    private static boolean ready;
    private static boolean changed;
    private static int ticks;

    private TerrainMap() {}

    public static void register(Minecraft mc) {
        texture = new DynamicTexture(() -> "Garnet terrain map", SIZE, SIZE, false);
        mc.getTextureManager().register(ID, texture);
        Arrays.fill(held, NOTHING);
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

    /** Where the wrap falls, so the shaders can find a column's texel. */
    public static float wrapX() {
        return Math.floorMod(originX, SIZE);
    }

    public static float wrapZ() {
        return Math.floorMod(originZ, SIZE);
    }

    /** Binds the map to a pass that declares a `TerrainMap` sampler. */
    public static void bind(com.mojang.renderpearl.api.commands.RenderPass pass) {
        if (texture == null || !ready) return;
        com.mojang.renderpearl.api.textures.GpuTextureView view = texture.getTextureView();
        if (view == null) return;
        com.mojang.renderpearl.api.textures.GpuSampler sampler = com.mojang.blaze3d.systems.RenderSystem.getSamplerCache()
                .getRepeat(com.mojang.renderpearl.api.textures.FilterMode.NEAREST);
        pass.setUniform("TerrainMapSampler", view, sampler);
    }

    /** Client tick: keep the window on the player and fill in what is missing. */
    public static void tick(Minecraft mc) {
        if (mc.level == null || mc.player == null || texture == null) {
            ready = false;
            return;
        }
        originX = (Mth.floor(mc.player.getX()) - SIZE / 2) & ~15;
        originZ = (Mth.floor(mc.player.getZ()) - SIZE / 2) & ~15;

        for (int i = 0; i < SWEEP_BUDGET; i++) {
            stale[sweep] = true;
            sweep = (sweep + 1) % stale.length;
        }

        int filled = 0;
        for (int scanned = 0; scanned < held.length && filled < FILL_BUDGET; scanned++) {
            int slot = (cursor + scanned) % held.length;
            long wanted = chunkFor(slot);
            if (!stale[slot] && held[slot] == wanted) continue;
            if (!fill(mc.level, slot, wanted)) continue; // not loaded yet; try later
            stale[slot] = false;
            held[slot] = wanted;
            changed = true;
            filled++;
            cursor = slot + 1;
        }

        if (changed && ++ticks % UPLOAD_EVERY_TICKS == 0) {
            texture.upload();
            changed = false;
            ready = true;
        }
    }

    /** The chunk that belongs in this texel block, given where the window is. */
    private static long chunkFor(int slot) {
        int cornerX = originX >> 4;
        int cornerZ = originZ >> 4;
        int x = cornerX + Math.floorMod(slot % CHUNKS - cornerX, CHUNKS);
        int z = cornerZ + Math.floorMod(slot / CHUNKS - cornerZ, CHUNKS);
        return (long) x << 32 | (z & 0xFFFFFFFFL);
    }

    /** Writes one chunk's columns into the map. False if it is not loaded. */
    private static boolean fill(ClientLevel level, int slot, long wanted) {
        LevelChunk chunk = level.getChunkSource().getChunk((int) (wanted >> 32), (int) wanted, ChunkStatus.FULL, false);
        if (chunk == null) return false;
        NativeImage image = texture.getPixels();
        if (image == null) return false;
        int baseX = slot % CHUNKS * 16;
        int baseZ = slot / CHUNKS * 16;
        int minY = level.getMinY();
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
                image.setPixelABGR(baseX + x, baseZ + z,
                        0xFF000000 | Math.min(255, water) << 16 | (encoded >> 8) << 8 | encoded & 0xFF);
            }
        }
        return true;
    }
}
