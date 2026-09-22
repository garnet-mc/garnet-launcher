package org.garnetmc.client;

import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Player;
import org.garnetmc.client.voice.VoiceClient;

import java.util.List;
import java.util.UUID;

/**
 * Small indicator in the bottom-left corner: whether voice is connected,
 * muted or transmitting, and the names of people talking right now.
 */
public final class VoiceHud {
    private static final Identifier ID = Identifier.fromNamespaceAndPath("garnet", "voice");
    private static final int GREEN = 0xFF5BE38A;
    private static final int RED = 0xFFE35B5B;
    private static final int GREY = 0xFF9A9A9A;
    private static final int WHITE = 0xFFFFFFFF;

    private VoiceHud() {}

    public static void register() {
        HudElementRegistry.addLast(ID, (graphics, delta) -> render(graphics));
    }

    private static void render(GuiGraphicsExtractor graphics) {
        Minecraft mc = Minecraft.getInstance();
        VoiceClient voice = GarnetClient.voice();
        if (voice == null || mc.player == null) return;

        int x = 4;
        int y = graphics.guiHeight() - 14;

        String label;
        int colour;
        if (!voice.isConnected()) {
            label = "voice: connecting";
            colour = GREY;
        } else if (voice.isMuted()) {
            label = "voice: muted";
            colour = RED;
        } else if (voice.isTransmitting()) {
            label = "voice: talking";
            colour = GREEN;
        } else {
            label = "voice: on";
            colour = WHITE;
        }
        graphics.fill(x - 2, y - 2, x + 6, y + 6, colour);
        graphics.text(mc.font, label, x + 10, y - 1, WHITE, true);

        List<UUID> talking = voice.talking();
        for (int i = 0; i < talking.size() && i < 6; i++) {
            Player other = mc.level == null ? null : mc.level.getPlayerByUUID(talking.get(i));
            Component name = other != null ? other.getName() : Component.literal("?");
            graphics.text(mc.font, name, x + 10, y - 12 * (i + 1) - 1, GREEN, true);
        }
    }
}
