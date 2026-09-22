package org.garnetmc.client.render;

import net.fabricmc.fabric.api.client.renderer.v1.mesh.MutableQuadView;
import net.minecraft.client.Minecraft;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.SugarCaneBlock;
import net.minecraft.world.level.block.VegetationBlock;
import net.minecraft.world.level.block.VineBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;

/**
 * Wind for grass, crops and leaves.
 *
 * The vertex shader cannot tell a fern from a fence post, so the mod marks
 * the plants while the chunk is being built: each vertex of a waving block
 * gets its colour's alpha dropped by a step or two, and terrain.vsh reads
 * that back as how far the vertex may sway. Alpha is 255 everywhere else,
 * which means no wind and a vertex that vanilla would have drawn anyway.
 *
 * Grass sways from the ground up, so the level follows the vertex's height
 * inside its block; the top half of a tall plant carries on where the lower
 * half stopped. Leaves and sugar cane sway as whole blocks instead, which
 * keeps stacked and adjacent blocks from tearing apart.
 */
public final class Wind {
    /** Highest sway level, and so the deepest dent we make in the alpha. */
    public static final int MAX_LEVEL = 7;

    private static final int NONE = 0;
    private static final int PLANT = 1;
    private static final int PLANT_TOP_HALF = 2;
    private static final int WHOLE_BLOCK = 3;

    // [0] what the block being built is, [1] are we inside a block at all,
    // [2] has the alpha been cleared for this block. Chunks are built on
    // several worker threads, so this has to be per thread.
    private static final ThreadLocal<int[]> state = ThreadLocal.withInitial(() -> new int[3]);

    private Wind() {}

    /** Chunk builder: starting on a block. */
    public static void beginBlock(BlockState block) {
        int[] s = state.get();
        s[0] = kindOf(block);
        s[1] = 1;
        s[2] = 0;
    }

    /** Chunk builder: done with that block. */
    public static void endBlock() {
        int[] s = state.get();
        s[0] = NONE;
        s[1] = 0;
    }

    /** Marks one quad of the block being built, if it is one that waves. */
    public static void markQuad(MutableQuadView quad) {
        int[] s = state.get();
        if (s[1] == 0) return; // not terrain: block entities, items, particles
        if (s[0] == NONE) {
            // Quads are reused between blocks, so a mark left over from the
            // last one has to be taken back off.
            if (s[2] == 0) {
                s[2] = 1;
                for (int i = 0; i < 4; i++) {
                    quad.color(i, quad.color(i) | 0xFF000000);
                }
            }
            return;
        }
        float strength = GarnetRender.settings.wind;
        float floor = (float) Math.floor(Math.min(Math.min(quad.y(0), quad.y(1)), Math.min(quad.y(2), quad.y(3))) + 0.001f);
        for (int i = 0; i < 4; i++) {
            float y = quad.y(i) - floor; // 0 at the bottom of the block, 1 at the top
            float reach = switch (s[0]) {
                case PLANT -> y * 3.0f;
                case PLANT_TOP_HALF -> (y + 1.0f) * 3.0f;
                default -> 1.4f;
            };
            int level = Math.round(reach * strength);
            level = Math.max(0, Math.min(MAX_LEVEL, level));
            quad.color(i, (quad.color(i) & 0x00FFFFFF) | ((255 - level) << 24));
        }
    }

    private static int kindOf(BlockState block) {
        if (GarnetRender.settings.wind <= 0.0f) return NONE;
        Block type = block.getBlock();
        if (type instanceof LeavesBlock || type instanceof VineBlock || type instanceof SugarCaneBlock) {
            return WHOLE_BLOCK;
        }
        if (type instanceof VegetationBlock) {
            boolean topHalf = block.hasProperty(BlockStateProperties.DOUBLE_BLOCK_HALF)
                    && block.getValue(BlockStateProperties.DOUBLE_BLOCK_HALF) == DoubleBlockHalf.UPPER;
            return topHalf ? PLANT_TOP_HALF : PLANT;
        }
        return NONE;
    }

    /** The marks are baked into the chunk meshes, so a change needs a rebuild. */
    public static void rebuildChunks() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) return;
        mc.levelRenderer.invalidateCompiledGeometry(mc.level, mc.options,
                ((org.garnetmc.client.mixin.GameRendererAccessor) mc.gameRenderer).garnet$mainCamera(), mc.getBlockColors());
    }
}
