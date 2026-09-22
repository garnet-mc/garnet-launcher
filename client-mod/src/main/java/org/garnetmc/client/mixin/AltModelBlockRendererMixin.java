package org.garnetmc.client.mixin;

import net.fabricmc.fabric.api.client.renderer.v1.mesh.MutableQuadView;
import net.fabricmc.fabric.api.client.renderer.v1.mesh.QuadEmitter;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import org.garnetmc.client.render.Wind;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Chunk meshes are built by Fabric's renderer, not vanilla's, so the wind
 * marks go on here: which block is being built, and then every quad of it
 * once the renderer has finished shading and tinting it.
 */
@Mixin(targets = "net.fabricmc.fabric.impl.client.indigo.renderer.render.AltModelBlockRendererImpl", remap = false)
abstract class AltModelBlockRendererMixin {
    @Inject(method = "tesselateBlock", at = @At("HEAD"), require = 0)
    private void garnet$beginBlock(QuadEmitter emitter, float x, float y, float z, BlockAndTintGetter level,
                                   BlockPos pos, BlockState block, BlockStateModel model, long seed, CallbackInfo ci) {
        Wind.beginBlock(block);
    }

    @Inject(method = "tesselateBlock", at = @At("RETURN"), require = 0)
    private void garnet$endBlock(QuadEmitter emitter, float x, float y, float z, BlockAndTintGetter level,
                                 BlockPos pos, BlockState block, BlockStateModel model, long seed, CallbackInfo ci) {
        Wind.endBlock();
    }

    @Inject(method = "transform", at = @At("RETURN"), require = 0)
    private void garnet$markQuad(MutableQuadView quad, CallbackInfoReturnable<Boolean> kept) {
        if (kept.getReturnValue()) {
            Wind.markQuad(quad);
        }
    }
}
