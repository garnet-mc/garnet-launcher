package org.garnetmc.client.voice;

import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.openal.AL10;
import org.lwjgl.openal.AL11;
import org.lwjgl.openal.ALC10;
import org.lwjgl.openal.EXTEfx;

/**
 * Turns the world around the listener into OpenAL parameters.
 *
 * Occlusion: a ray from the listener to the speaker is sampled every half
 * block; every solid block it passes through muffles the voice a bit more
 * (gain down, low-pass filter tighter). Talking through a one block wall
 * still works, shouting through a mountain does not.
 *
 * Reverb: rays are cast from the listener in a handful of directions. Short
 * rays plus no sky light means "we are in a cave or a room", which drives a
 * single shared EFX reverb effect that every speaker feeds into.
 */
public final class Acoustics {
    private static final Vec3[] PROBES = {
            new Vec3(1, 0, 0), new Vec3(-1, 0, 0), new Vec3(0, 0, 1), new Vec3(0, 0, -1),
            new Vec3(0, 1, 0), new Vec3(0, -1, 0),
            new Vec3(0.7, 0.4, 0.7), new Vec3(-0.7, 0.4, -0.7), new Vec3(0.7, 0.4, -0.7), new Vec3(-0.7, 0.4, 0.7),
    };
    private static final double PROBE_LENGTH = 24.0;

    private static boolean efxChecked;
    private static boolean efxAvailable;
    private static int effectSlot;
    private static int reverbEffect;
    private static float currentDecay = -1f;

    private Acoustics() {}

    /** 0 = clear line of sight, 1 = fully buried. */
    public static float occlusion(Level level, Vec3 from, Vec3 to) {
        Vec3 delta = to.subtract(from);
        double length = delta.length();
        if (length < 0.001) return 0f;
        Vec3 step = delta.scale(0.5 / length);
        int solid = 0;
        int samples = (int) Math.ceil(length / 0.5);
        Vec3 point = from;
        BlockPos last = null;
        for (int i = 0; i < samples; i++) {
            point = point.add(step);
            BlockPos pos = BlockPos.containing(point);
            if (pos.equals(last)) continue;
            last = pos;
            if (level.getBlockState(pos).canOcclude()) {
                solid++;
                if (solid >= 6) break;
            }
        }
        // Each block of material takes a chunk off; six blocks is a wall.
        return Math.min(1f, solid / 6f);
    }

    /** 0 = open sky, 1 = deep underground / tight room. */
    public static float enclosure(Level level, Vec3 listener) {
        float closed = 0f;
        for (Vec3 dir : PROBES) {
            float hit = 1f;
            Vec3 point = listener;
            Vec3 step = dir.normalize().scale(0.5);
            for (double d = 0; d < PROBE_LENGTH; d += 0.5) {
                point = point.add(step);
                if (level.getBlockState(BlockPos.containing(point)).canOcclude()) {
                    hit = (float) (d / PROBE_LENGTH);
                    break;
                }
            }
            closed += 1f - hit;
        }
        closed /= PROBES.length;
        int sky = level.getBrightness(LightLayer.SKY, BlockPos.containing(listener));
        float noSky = 1f - sky / 15f;
        return Math.max(0f, Math.min(1f, closed * 0.7f + noSky * 0.3f));
    }

    /** Called once per tick from the client thread. */
    public static void tickReverb(Minecraft mc) {
        if (!efx() || mc.level == null || mc.player == null) return;
        float enclosure = enclosure(mc.level, mc.player.getEyePosition());
        // Open air barely echoes; a cave rings for a couple of seconds.
        float decay = 0.2f + enclosure * enclosure * 3.3f;
        if (Math.abs(decay - currentDecay) < 0.05f) return;
        currentDecay = decay;
        EXTEfx.alEffectf(reverbEffect, EXTEfx.AL_REVERB_DECAY_TIME, decay);
        EXTEfx.alEffectf(reverbEffect, EXTEfx.AL_REVERB_GAIN, 0.05f + enclosure * 0.4f);
        EXTEfx.alEffectf(reverbEffect, EXTEfx.AL_REVERB_LATE_REVERB_GAIN, 0.3f + enclosure * 1.0f);
        EXTEfx.alEffectf(reverbEffect, EXTEfx.AL_REVERB_ROOM_ROLLOFF_FACTOR, 0.5f);
        EXTEfx.alAuxiliaryEffectSloti(effectSlot, EXTEfx.AL_EFFECTSLOT_EFFECT, reverbEffect);
    }

    /** Creates a low-pass filter for one source, or 0 when EFX is missing. */
    static int newLowPassFilter() {
        if (!efx()) return 0;
        int filter = EXTEfx.alGenFilters();
        EXTEfx.alFilteri(filter, EXTEfx.AL_FILTER_TYPE, EXTEfx.AL_FILTER_LOWPASS);
        return filter;
    }

    static void applyOcclusion(int source, int filter, float occlusion) {
        if (filter == 0) return;
        EXTEfx.alFilterf(filter, EXTEfx.AL_LOWPASS_GAIN, 1f - occlusion * 0.6f);
        EXTEfx.alFilterf(filter, EXTEfx.AL_LOWPASS_GAINHF, 1f - occlusion * 0.95f);
        AL10.alSourcei(source, EXTEfx.AL_DIRECT_FILTER, filter);
        AL11.alSource3i(source, EXTEfx.AL_AUXILIARY_SEND_FILTER, effectSlot, 0, EXTEfx.AL_FILTER_NULL);
    }

    static void deleteFilter(int filter) {
        if (filter != 0) EXTEfx.alDeleteFilters(filter);
    }

    private static boolean efx() {
        if (efxChecked) return efxAvailable;
        efxChecked = true;
        long context = ALC10.alcGetCurrentContext();
        if (context == 0) return false;
        long device = ALC10.alcGetContextsDevice(context);
        efxAvailable = ALC10.alcIsExtensionPresent(device, "ALC_EXT_EFX");
        if (!efxAvailable) return false;
        effectSlot = EXTEfx.alGenAuxiliaryEffectSlots();
        reverbEffect = EXTEfx.alGenEffects();
        EXTEfx.alEffecti(reverbEffect, EXTEfx.AL_EFFECT_TYPE, EXTEfx.AL_EFFECT_REVERB);
        EXTEfx.alAuxiliaryEffectSloti(effectSlot, EXTEfx.AL_EFFECTSLOT_EFFECT, reverbEffect);
        return true;
    }
}
