package org.garnetmc.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientConfigurationNetworking;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.network.chat.Component;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import org.garnetmc.client.gui.GarnetButtons;
import org.garnetmc.client.render.GarnetRender;
import org.garnetmc.client.voice.VoiceClient;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.nio.charset.StandardCharsets;

/**
 * Client-side half of Garnet.
 *
 * Two plugin channels:
 *   garnet:mods   – the server sends the mods it wants; we answer with what
 *                   is installed so it can let us in (or explain what is
 *                   missing).
 *   garnet:voice  – the server hands us a UDP port and a one-time secret for
 *                   proximity voice chat.
 */
public final class GarnetClient implements ClientModInitializer {
    public static final Logger LOG = LoggerFactory.getLogger("garnet");
    public static final Identifier MODS_CHANNEL = Identifier.fromNamespaceAndPath("garnet", "mods");
    public static final Identifier VOICE_CHANNEL = Identifier.fromNamespaceAndPath("garnet", "voice");

    /** Raw bytes on a channel; both directions use the same shape. */
    public record RawPayload(Identifier channel, byte[] data) implements CustomPacketPayload {
        public static Type<RawPayload> type(Identifier channel) {
            return new Type<>(channel);
        }

        public static StreamCodec<FriendlyByteBuf, RawPayload> codec(Identifier channel) {
            return StreamCodec.of(
                    (buf, payload) -> buf.writeBytes(payload.data),
                    buf -> {
                        byte[] bytes = new byte[buf.readableBytes()];
                        buf.readBytes(bytes);
                        return new RawPayload(channel, bytes);
                    });
        }

        @Override
        public Type<? extends CustomPacketPayload> type() {
            return type(channel);
        }
    }

    public static final CustomPacketPayload.Type<RawPayload> MODS_TYPE = RawPayload.type(MODS_CHANNEL);
    public static final CustomPacketPayload.Type<RawPayload> VOICE_TYPE = RawPayload.type(VOICE_CHANNEL);

    private static VoiceClient voice;

    @Override
    public void onInitializeClient() {
        // Register the channels for the configuration phase (mods handshake)
        // and the play phase (voice, and mods again if a server asks late).
        PayloadTypeRegistry.clientboundConfiguration().register(MODS_TYPE, RawPayload.codec(MODS_CHANNEL));
        PayloadTypeRegistry.serverboundConfiguration().register(MODS_TYPE, RawPayload.codec(MODS_CHANNEL));
        PayloadTypeRegistry.clientboundPlay().register(MODS_TYPE, RawPayload.codec(MODS_CHANNEL));
        PayloadTypeRegistry.serverboundPlay().register(MODS_TYPE, RawPayload.codec(MODS_CHANNEL));
        PayloadTypeRegistry.clientboundPlay().register(VOICE_TYPE, RawPayload.codec(VOICE_CHANNEL));

        ClientConfigurationNetworking.registerGlobalReceiver(MODS_TYPE, (payload, context) -> {
            context.responseSender().sendPacket(new RawPayload(MODS_CHANNEL, installedModsReport()));
        });
        ClientPlayNetworking.registerGlobalReceiver(MODS_TYPE, (payload, context) -> {
            context.responseSender().sendPacket(new RawPayload(MODS_CHANNEL, installedModsReport()));
        });
        ClientPlayNetworking.registerGlobalReceiver(VOICE_TYPE, (payload, context) -> {
            context.client().execute(() -> startVoice(payload.data()));
        });
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> stopVoice());

        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            if (voice != null) voice.tick();
            GarnetRender.tick(client);
        });

        GarnetRender.init();
        GarnetButtons.register();
        Keybinds.register();
        VoiceHud.register();
        LOG.info("Garnet client ready");
    }

    /** JSON the server expects on garnet:mods: {"installed":[{"id","version"}]}. */
    static byte[] installedModsReport() {
        JsonArray installed = new JsonArray();
        FabricLoader.getInstance().getAllMods().forEach(mod -> {
            JsonObject entry = new JsonObject();
            entry.addProperty("id", mod.getMetadata().getId());
            entry.addProperty("version", mod.getMetadata().getVersion().getFriendlyString());
            installed.add(entry);
        });
        JsonObject report = new JsonObject();
        report.add("installed", installed);
        report.addProperty("client", "garnet");
        return report.toString().getBytes(StandardCharsets.UTF_8);
    }

    /** garnet:voice payload: protocol version (u8), UDP port (u16), secret (16 bytes). */
    private static void startVoice(byte[] data) {
        if (data.length < 19) {
            LOG.warn("garnet:voice payload too short");
            return;
        }
        int version = data[0] & 0xFF;
        int port = ((data[1] & 0xFF) << 8) | (data[2] & 0xFF);
        byte[] secret = new byte[16];
        System.arraycopy(data, 3, secret, 0, 16);
        if (version != VoiceClient.PROTOCOL_VERSION) {
            LOG.warn("server voice protocol {} does not match ours ({})", version, VoiceClient.PROTOCOL_VERSION);
            return;
        }
        stopVoice();
        Minecraft mc = Minecraft.getInstance();
        if (mc.getCurrentServer() == null || mc.player == null) {
            return;
        }
        String host = mc.getCurrentServer().ip.split(":")[0];
        try {
            voice = new VoiceClient(host, port, secret, mc.player.getUUID());
            voice.start();
            LOG.info("voice chat connected to {}:{}", host, port);
            String name = mc.getCurrentServer().name;
            SystemToast.add(mc.gui.toastManager(), new SystemToast.SystemToastId(),
                    Component.literal("Garnet server").withStyle(style -> style.withColor(0xE04060)),
                    Component.literal(name + " · voice chat ready · K for Garnet Render"));
        } catch (Exception e) {
            LOG.error("could not start voice chat", e);
        }
    }

    private static void stopVoice() {
        if (voice != null) {
            voice.close();
            voice = null;
        }
    }

    public static VoiceClient voice() {
        return voice;
    }

    /** Whether the server manifest says voice is on; used by the HUD. */
    public static boolean voiceConnected() {
        return voice != null && voice.isConnected();
    }

    static JsonObject parseJson(byte[] data) {
        return JsonParser.parseString(new String(data, StandardCharsets.UTF_8)).getAsJsonObject();
    }
}
