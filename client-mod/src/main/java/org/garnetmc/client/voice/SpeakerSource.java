package org.garnetmc.client.voice;

import de.maxhenkel.opus4j.OpusDecoder;
import net.minecraft.client.Minecraft;
import net.minecraft.world.phys.Vec3;
import org.lwjgl.openal.AL10;

import java.util.UUID;

/**
 * One OpenAL streaming source per player we can hear. Minecraft already keeps
 * the OpenAL listener at the camera every frame, so placing the source at the
 * speaker's world position is enough for panning and distance falloff; the
 * game's own sound engine and ours share the same context.
 */
final class SpeakerSource implements AutoCloseable {
    private static final int MAX_QUEUED = 12; // 240 ms of audio before we start dropping

    final UUID player;
    private final OpusDecoder decoder;
    private final int source;
    private final int filter;

    private volatile float x, y, z;
    private volatile boolean whisper;
    private volatile float range;
    private int lastSeq = -1;
    private volatile long lastPacketAt = System.currentTimeMillis();

    SpeakerSource(UUID player, int sampleRate) {
        this.player = player;
        this.decoder = VoiceClient.newDecoder();
        this.source = AL10.alGenSources();
        this.filter = Acoustics.newLowPassFilter();
        AL10.alSourcei(source, AL10.AL_SOURCE_RELATIVE, AL10.AL_FALSE);
        AL10.alSourcef(source, AL10.AL_ROLLOFF_FACTOR, 1.0f);
        AL10.alSourcef(source, AL10.AL_REFERENCE_DISTANCE, 2.0f);
        AL10.alSourcei(source, AL10.AL_LOOPING, AL10.AL_FALSE);
    }

    /** Called from the network thread with a freshly received frame. */
    void queue(int seq, byte[] opus, float x, float y, float z, boolean whisper, float range) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.whisper = whisper;
        this.range = range;
        lastPacketAt = System.currentTimeMillis();

        // Fill in a single lost frame with packet loss concealment so a
        // dropped packet is a wobble rather than a click.
        if (lastSeq >= 0 && seq == lastSeq + 2) {
            short[] lost = decoder.decode(null);
            if (lost != null) push(lost);
        }
        lastSeq = seq;
        short[] decoded = decoder.decode(opus);
        if (decoded != null) push(decoded);
    }

    private void push(short[] samples) {
        reclaim();
        int queued = AL10.alGetSourcei(source, AL10.AL_BUFFERS_QUEUED);
        if (queued >= MAX_QUEUED) return;
        int buffer = AL10.alGenBuffers();
        AL10.alBufferData(buffer, AL10.AL_FORMAT_MONO16, samples, VoiceClient.SAMPLE_RATE);
        AL10.alSourceQueueBuffers(source, buffer);
        AL10.alSource3f(source, AL10.AL_POSITION, x, y, z);
        if (AL10.alGetSourcei(source, AL10.AL_SOURCE_STATE) != AL10.AL_PLAYING && queued >= 2) {
            AL10.alSourcePlay(source);
        }
    }

    /** Deletes buffers OpenAL has finished with. */
    private void reclaim() {
        int processed = AL10.alGetSourcei(source, AL10.AL_BUFFERS_PROCESSED);
        while (processed-- > 0) {
            int buffer = AL10.alSourceUnqueueBuffers(source);
            AL10.alDeleteBuffers(buffer);
        }
    }

    /** Called every client tick on the client thread. */
    void update(Minecraft mc) {
        Vec3 listener = mc.player.getEyePosition();
        Vec3 speaker = new Vec3(x, y, z);
        float occlusion = Acoustics.occlusion(mc.level, listener, speaker);
        float maxDistance = whisper ? Math.min(range, 8f) : range;
        AL10.alSourcef(source, AL10.AL_MAX_DISTANCE, maxDistance);
        AL10.alSourcef(source, AL10.AL_GAIN, (whisper ? 0.6f : 1.0f) * (1f - occlusion * 0.5f));
        AL10.alSource3f(source, AL10.AL_POSITION, x, y, z);
        Acoustics.applyOcclusion(source, filter, occlusion);
    }

    boolean isTalking() {
        return System.currentTimeMillis() - lastPacketAt < 250;
    }

    @Override
    public void close() {
        AL10.alSourceStop(source);
        reclaim();
        AL10.alDeleteSources(source);
        Acoustics.deleteFilter(filter);
        decoder.close();
    }
}
