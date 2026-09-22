package org.garnetmc.client.mixin;

import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** "Minecraft* 26.3 - ..." becomes "Garnet - Minecraft 26.3 - ...". */
@Mixin(Minecraft.class)
abstract class MinecraftTitleMixin {
    @Inject(method = "createTitle", at = @At("RETURN"), cancellable = true)
    private void garnet$brandTitle(CallbackInfoReturnable<String> cir) {
        cir.setReturnValue("Garnet - " + cir.getReturnValue().replace("Minecraft*", "Minecraft"));
    }
}
