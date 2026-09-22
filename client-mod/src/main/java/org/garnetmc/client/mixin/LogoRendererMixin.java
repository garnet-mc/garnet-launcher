package org.garnetmc.client.mixin;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.LogoRenderer;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The title screen shows the Garnet mark instead of the Minecraft logo. */
@Mixin(LogoRenderer.class)
abstract class LogoRendererMixin {
    private static final Identifier GARNET_TITLE = Identifier.fromNamespaceAndPath("garnet", "textures/gui/title.png");
    private static final int WIDTH = 256;
    private static final int HEIGHT = 64;

    @Inject(method = "extractRenderState(Lnet/minecraft/client/gui/GuiGraphicsExtractor;IFI)V", at = @At("HEAD"), cancellable = true)
    private void garnet$drawLogo(GuiGraphicsExtractor graphics, int screenWidth, float alpha, int y, CallbackInfo ci) {
        int x = screenWidth / 2 - WIDTH / 2;
        graphics.blit(RenderPipelines.GUI_TEXTURED, GARNET_TITLE, x, y - 8, 0f, 0f, WIDTH, HEIGHT, 1024, 256, 1024, 256, ARGB.white(alpha));
        ci.cancel();
    }
}
