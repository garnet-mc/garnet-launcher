package org.garnetmc.client.mixin;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.resource.GraphicsResourceAllocator;
import net.minecraft.client.renderer.PostChain;
import org.garnetmc.client.render.GarnetRender;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Once per frame, before our chain runs, write the frame block all its passes share. */
@Mixin(PostChain.class)
abstract class PostChainMixin {
    @Inject(method = "process", at = @At("HEAD"))
    private void garnet$writeFrame(RenderTarget target, GraphicsResourceAllocator allocator, CallbackInfo ci) {
        if (GarnetRender.PIPELINE.equals(((PostChain) (Object) this).id())) {
            GarnetRender.writeFrame();
        }
    }
}
