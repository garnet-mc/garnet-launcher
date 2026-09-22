package org.garnetmc.client.render;

import net.fabricmc.loader.api.FabricLoader;
import org.garnetmc.client.GarnetClient;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Properties;

/** Strengths for each effect, 0 = off, 1 = default look. Saved in config/garnet-render.properties. */
public final class RenderSettings {
    public boolean enabled = true;
    public float ambientOcclusion = 1.0f;
    public float shadows = 1.0f;
    public float bloom = 1.0f;
    public float exposure = 1.0f;
    public float lightShafts = 1.0f;
    public float water = 1.0f;

    private static Path file() {
        return FabricLoader.getInstance().getConfigDir().resolve("garnet-render.properties");
    }

    public static RenderSettings load() {
        RenderSettings s = new RenderSettings();
        Path path = file();
        if (!Files.exists(path)) return s;
        Properties p = new Properties();
        try (Reader reader = Files.newBufferedReader(path)) {
            p.load(reader);
        } catch (IOException e) {
            GarnetClient.LOG.warn("could not read {}", path, e);
            return s;
        }
        s.enabled = Boolean.parseBoolean(p.getProperty("enabled", "true"));
        s.ambientOcclusion = number(p, "ambient_occlusion", 1.0f);
        s.shadows = number(p, "shadows", 1.0f);
        s.bloom = number(p, "bloom", 1.0f);
        s.exposure = number(p, "exposure", 1.0f);
        s.lightShafts = number(p, "light_shafts", 1.0f);
        s.water = number(p, "water", 1.0f);
        return s;
    }

    public void save() {
        Properties p = new Properties();
        p.setProperty("enabled", Boolean.toString(enabled));
        p.setProperty("ambient_occlusion", Float.toString(ambientOcclusion));
        p.setProperty("shadows", Float.toString(shadows));
        p.setProperty("bloom", Float.toString(bloom));
        p.setProperty("exposure", Float.toString(exposure));
        p.setProperty("light_shafts", Float.toString(lightShafts));
        p.setProperty("water", Float.toString(water));
        try (Writer writer = Files.newBufferedWriter(file())) {
            p.store(writer, "Garnet Render. Each strength is 0 (off) to about 2; 1 is the default look.");
        } catch (IOException e) {
            GarnetClient.LOG.warn("could not save render settings", e);
        }
    }

    private static float number(Properties p, String key, float fallback) {
        try {
            return Float.parseFloat(p.getProperty(key, Float.toString(fallback)));
        } catch (NumberFormatException e) {
            return fallback;
        }
    }
}
