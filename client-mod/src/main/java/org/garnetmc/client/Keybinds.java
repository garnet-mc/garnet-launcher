package org.garnetmc.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.minecraft.client.KeyMapping;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import org.garnetmc.client.voice.VoiceClient;

/** V to talk (hold), B to toggle voice on/off, N to whisper (hold). */
public final class Keybinds {
    private static final KeyMapping.Category CATEGORY = KeyMapping.Category.register(Identifier.fromNamespaceAndPath("garnet", "voice"));

    public static KeyMapping pushToTalk;
    public static KeyMapping toggleMute;
    public static KeyMapping whisper;

    private Keybinds() {}

    public static void register() {
        pushToTalk = KeyMappingHelper.registerKeyMapping(new KeyMapping("key.garnet.talk", InputConstants.Type.KEYBOARD, InputConstants.KEY_V, CATEGORY));
        toggleMute = KeyMappingHelper.registerKeyMapping(new KeyMapping("key.garnet.mute", InputConstants.Type.KEYBOARD, InputConstants.KEY_B, CATEGORY));
        whisper = KeyMappingHelper.registerKeyMapping(new KeyMapping("key.garnet.whisper", InputConstants.Type.KEYBOARD, InputConstants.KEY_N, CATEGORY));

        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            VoiceClient voice = GarnetClient.voice();
            while (toggleMute.consumeClick()) {
                if (voice != null) {
                    voice.setMuted(!voice.isMuted());
                    if (client.player != null) {
                        client.player.sendOverlayMessage(Component.literal(voice.isMuted() ? "Voice muted" : "Voice on"));
                    }
                }
            }
            if (voice != null) {
                voice.setTransmitting(pushToTalk.isDown() || whisper.isDown(), whisper.isDown());
            }
        });
    }
}
