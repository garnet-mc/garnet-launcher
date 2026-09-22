package org.garnetmc.client.mixin;

import com.llamalad7.mixinextras.sugar.Local;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import net.minecraft.client.renderer.PostPass;
import org.garnetmc.client.render.GarnetRender;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Vanilla post passes only know static uniforms from their JSON. Right before
 * one of our passes draws, hand it the per-frame block (matrices, sun, fog)
 * that PostChainMixin wrote at the start of the frame.
 */
@Mixin(PostPass.class)
abstract class PostPassMixin {
    @Shadow @Final private RenderPipeline pipeline;

    @Inject(method = "lambda$addToFrame$1", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/commands/RenderPass;draw(IIII)V"))
    private void garnet$bindFrame(CallbackInfo ci, @Local RenderPass pass) {
        if (GarnetRender.NAMESPACE.equals(pipeline.getLocation().getNamespace())) {
            GarnetRender.bindFrame(pass);
            String path = pipeline.getLocation().getPath();
            if (path.contains("lighting") || path.contains("composite")) {
                GarnetRender.bindTerrainMap(pass);
            }
        }
    }
}
