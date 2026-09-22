package org.garnetmc.client.gui;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import org.garnetmc.client.GarnetClient;
import org.garnetmc.client.render.GarnetRender;
import org.garnetmc.client.render.RenderSettings;
import org.garnetmc.client.voice.VoiceClient;

import java.util.function.DoubleConsumer;
import java.util.function.DoubleSupplier;

/** The Garnet menu: render settings, voice, and what server you are on. */
public final class GarnetScreen extends Screen {
    private static final Identifier TITLE = Identifier.fromNamespaceAndPath("garnet", "textures/gui/title.png");
    private static final int GARNET_RED = 0xFFE04060;
    private final Screen parent;
    private Button renderToggle;
    private Button voiceToggle;

    public GarnetScreen(Screen parent) {
        super(Component.literal("Garnet"));
        this.parent = parent;
    }

    @Override
    protected void init() {
        RenderSettings s = GarnetRender.settings;
        int w = 200;
        int x = width / 2 - w / 2;
        int y = 66;

        renderToggle = addRenderableWidget(Button.builder(renderLabel(), b -> {
            GarnetRender.toggle();
            b.setMessage(renderLabel());
        }).bounds(x, y, w, 20).build());
        y += 24;
        addRenderableWidget(slider(x, y, w, "Ambient occlusion", () -> s.ambientOcclusion, v -> s.ambientOcclusion = (float) v));
        addRenderableWidget(slider(x, y + 22, w, "Shadows", () -> s.shadows, v -> s.shadows = (float) v));
        addRenderableWidget(slider(x, y + 44, w, "Light shafts", () -> s.lightShafts, v -> s.lightShafts = (float) v));
        addRenderableWidget(slider(x, y + 66, w, "Bloom", () -> s.bloom, v -> s.bloom = (float) v));
        addRenderableWidget(slider(x, y + 88, w, "Water", () -> s.water, v -> s.water = (float) v));
        addRenderableWidget(slider(x, y + 110, w, "Exposure", () -> s.exposure, v -> s.exposure = (float) v));
        y += 136;

        voiceToggle = addRenderableWidget(Button.builder(voiceLabel(), b -> {
            VoiceClient voice = GarnetClient.voice();
            if (voice != null) voice.setMuted(!voice.isMuted());
            b.setMessage(voiceLabel());
        }).bounds(x, y, w, 20).build());
        voiceToggle.active = GarnetClient.voice() != null;
        y += 26;

        addRenderableWidget(Button.builder(Component.literal("Done"), b -> onClose()).bounds(x, y, w, 20).build());
    }

    private static Component renderLabel() {
        return Component.literal("Garnet Render: " + (GarnetRender.isEnabled() ? "On" : "Off"));
    }

    private static Component voiceLabel() {
        VoiceClient voice = GarnetClient.voice();
        if (voice == null) return Component.literal("Voice: not on a Garnet server");
        return Component.literal("Voice: " + (voice.isMuted() ? "Muted" : "On"));
    }

    /** A 0..2 slider where 1 is the default look. */
    private static AbstractSliderButton slider(int x, int y, int w, String name, DoubleSupplier get, DoubleConsumer set) {
        return new AbstractSliderButton(x, y, w, 20, Component.literal(name), get.getAsDouble() / 2.0) {
            {
                updateMessage();
            }

            @Override
            protected void updateMessage() {
                int percent = (int) Math.round(value * 200.0);
                setMessage(Component.literal(name + ": " + (percent == 0 ? "Off" : percent + "%")));
            }

            @Override
            protected void applyValue() {
                set.accept(Math.round(value * 40.0) / 20.0);
            }
        };
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partial) {
        super.extractRenderState(graphics, mouseX, mouseY, partial);
        int logoW = 144;
        int logoH = 36;
        graphics.blit(RenderPipelines.GUI_TEXTURED, TITLE, width / 2 - logoW / 2, 6, 0f, 0f, logoW, logoH, 1024, 256, 1024, 256, ARGB.white(1f));

        Minecraft mc = Minecraft.getInstance();
        String server = mc.getCurrentServer() != null ? mc.getCurrentServer().name + " (" + mc.getCurrentServer().ip + ")" : "Not connected";
        String line = "Server: " + server + (GarnetClient.voice() != null ? " · Garnet server, voice ready" : "");
        graphics.text(font, line, width / 2 - font.width(line) / 2, 44, 0xFFBBBBBB, true);
        String keys = "K toggles Garnet Render · V talk · N whisper · B mute";
        graphics.text(font, keys, width / 2 - font.width(keys) / 2, 54, 0xFF888888, true);
    }

    @Override
    public void onClose() {
        GarnetRender.settings.save();
        minecraft.gui.setScreen(parent);
    }
}
