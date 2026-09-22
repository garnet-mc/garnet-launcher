package org.garnetmc.client.gui;

import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.Component;
import org.garnetmc.client.GarnetClient;

/** Puts a "Garnet" button in the corner of the title and pause screens. */
public final class GarnetButtons {
    private GarnetButtons() {}

    public static void register() {
        ScreenEvents.AFTER_INIT.register((client, screen, width, height) -> {
            if (screen instanceof TitleScreen || screen instanceof PauseScreen) {
                Screens.getWidgets(screen).add(Button.builder(Component.literal("Garnet"), b -> {
                    GarnetClient.LOG.info("opening the Garnet menu");
                    client.gui.setScreen(new GarnetScreen(screen));
                }).bounds(width - 66, 6, 60, 20).build());
            }
        });
    }
}
