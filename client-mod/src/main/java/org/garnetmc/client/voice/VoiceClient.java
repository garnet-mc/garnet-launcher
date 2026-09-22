package org.garnetmc.client.voice;

import de.maxhenkel.opus4j.OpusDecoder;
import de.maxhenkel.opus4j.OpusEncoder;
import net.minecraft.client.Minecraft;
import org.garnetmc.client.GarnetClient;

import javax.sound.sampled.AudioFormat;
import javax.sound.sampled.AudioSystem;
import javax.sound.sampled.DataLine;
import javax.sound.sampled.TargetDataLine;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.nio.ByteBuffer;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Talks to the Garnet voice relay (see garnet-voice in the server repo).
 *
 * Packet layout, big-endian, first byte is the type:
 *   0x01 auth       uuid[16] secret[16]
 *   0x02 audio      seq u32, flags u8 (bit0 whisper), len u16, opus[len]
 *   0x03 ping       nonce u64
 *   0x04 leave
 *   0x81 auth ok    range f32
 *   0x82 audio      speaker uuid[16], seq u32, flags u8, x f32, y f32, z f32, len u16, opus[len]
 *   0x84 gone       speaker uuid[16]
 *   0x8F error      u16 len + utf8
 *
 * Audio is 48 kHz mono, 20 ms frames (960 samples). Each speaker gets a
 * positional OpenAL source (see {@link SpeakerSource}), so distance, panning,
 * occlusion and reverb all happen locally where the world is known.
 */
public final class VoiceClient implements AutoCloseable {
    public static final int PROTOCOL_VERSION = 1;
    public static final int SAMPLE_RATE = 48000;
    public static final int FRAME_SAMPLES = 960;

    private final DatagramSocket socket;
    private final InetAddress host;
    private final int port;
    private final byte[] secret;
    private final UUID self;
    private final Map<UUID, SpeakerSource> speakers = new ConcurrentHashMap<>();

    private volatile boolean connected;
    private volatile boolean muted;
    private volatile boolean transmitting;
    private volatile boolean whispering;
    private volatile float range = 48f;
    private volatile boolean running = true;
    private int sequence;

    private Thread receiveThread;
    private Thread captureThread;
    private Thread pingThread;

    public VoiceClient(String host, int port, byte[] secret, UUID self) throws IOException {
        this.socket = new DatagramSocket();
        this.socket.setSoTimeout(1000);
        this.host = InetAddress.getByName(host);
        this.port = port;
        this.secret = secret;
        this.self = self;
    }

    public void start() throws IOException {
        sendAuth();
        receiveThread = new Thread(this::receiveLoop, "garnet-voice-rx");
        receiveThread.setDaemon(true);
        receiveThread.start();
        captureThread = new Thread(this::captureLoop, "garnet-voice-mic");
        captureThread.setDaemon(true);
        captureThread.start();
        pingThread = new Thread(this::pingLoop, "garnet-voice-ping");
        pingThread.setDaemon(true);
        pingThread.start();
    }

    private void send(byte[] data) {
        try {
            socket.send(new DatagramPacket(data, data.length, host, port));
        } catch (IOException e) {
            GarnetClient.LOG.debug("voice send failed", e);
        }
    }

    private void sendAuth() {
        ByteBuffer buf = ByteBuffer.allocate(33);
        buf.put((byte) 0x01);
        buf.putLong(self.getMostSignificantBits());
        buf.putLong(self.getLeastSignificantBits());
        buf.put(secret);
        send(buf.array());
    }

    private void pingLoop() {
        while (running) {
            try {
                Thread.sleep(5000);
            } catch (InterruptedException e) {
                return;
            }
            ByteBuffer buf = ByteBuffer.allocate(9);
            buf.put((byte) 0x03).putLong(System.nanoTime());
            send(buf.array());
            if (!connected) {
                sendAuth();
            }
        }
    }

    private void receiveLoop() {
        byte[] data = new byte[1500];
        DatagramPacket packet = new DatagramPacket(data, data.length);
        while (running) {
            try {
                socket.receive(packet);
            } catch (java.net.SocketTimeoutException e) {
                continue;
            } catch (IOException e) {
                if (running) GarnetClient.LOG.debug("voice receive failed", e);
                continue;
            }
            ByteBuffer buf = ByteBuffer.wrap(packet.getData(), 0, packet.getLength());
            int type = buf.get() & 0xFF;
            switch (type) {
                case 0x81 -> {
                    range = buf.getFloat();
                    connected = true;
                    GarnetClient.LOG.info("voice: authenticated, range {} blocks", range);
                }
                case 0x82 -> handleAudio(buf);
                case 0x84 -> {
                    UUID gone = new UUID(buf.getLong(), buf.getLong());
                    SpeakerSource source = speakers.remove(gone);
                    if (source != null) source.close();
                }
                case 0x8F -> {
                    int len = buf.getShort() & 0xFFFF;
                    byte[] text = new byte[len];
                    buf.get(text);
                    GarnetClient.LOG.warn("voice server: {}", new String(text, java.nio.charset.StandardCharsets.UTF_8));
                }
                default -> {}
            }
        }
    }

    private void handleAudio(ByteBuffer buf) {
        UUID speaker = new UUID(buf.getLong(), buf.getLong());
        int seq = buf.getInt();
        int flags = buf.get() & 0xFF;
        float x = buf.getFloat();
        float y = buf.getFloat();
        float z = buf.getFloat();
        int len = buf.getShort() & 0xFFFF;
        byte[] opus = new byte[len];
        buf.get(opus);
        SpeakerSource source = speakers.computeIfAbsent(speaker, id -> new SpeakerSource(id, SAMPLE_RATE));
        source.queue(seq, opus, x, y, z, (flags & 1) != 0, range);
    }

    /** Microphone -> Opus -> server, only while push-to-talk is held. */
    private void captureLoop() {
        AudioFormat format = new AudioFormat(SAMPLE_RATE, 16, 1, true, false);
        TargetDataLine line;
        try {
            line = (TargetDataLine) AudioSystem.getLine(new DataLine.Info(TargetDataLine.class, format));
            line.open(format, FRAME_SAMPLES * 2 * 4);
            line.start();
        } catch (Exception e) {
            GarnetClient.LOG.error("no microphone available for voice chat", e);
            return;
        }
        OpusEncoder encoder;
        try {
            encoder = new OpusEncoder(SAMPLE_RATE, 1, OpusEncoder.Application.VOIP);
            encoder.setMaxPayloadSize(1024);
            encoder.setMaxPacketLossPercentage(0.1f);
        } catch (Exception e) {
            GarnetClient.LOG.error("could not create the Opus encoder", e);
            line.close();
            return;
        }
        byte[] pcmBytes = new byte[FRAME_SAMPLES * 2];
        short[] pcm = new short[FRAME_SAMPLES];
        while (running) {
            int read = line.read(pcmBytes, 0, pcmBytes.length);
            if (read < pcmBytes.length) continue;
            if (!transmitting || muted || !connected) continue;
            ByteBuffer.wrap(pcmBytes).order(java.nio.ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(pcm);
            byte[] encoded = encoder.encode(pcm);
            ByteBuffer out = ByteBuffer.allocate(1 + 4 + 1 + 2 + encoded.length);
            out.put((byte) 0x02).putInt(sequence++).put((byte) (whispering ? 1 : 0)).putShort((short) encoded.length).put(encoded);
            send(out.array());
        }
        encoder.close();
        line.close();
    }

    /** Called from the render thread: keeps every speaker's source in sync with the world. */
    public void tick() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || mc.player == null) return;
        speakers.values().forEach(source -> source.update(mc));
        Acoustics.tickReverb(mc);
    }

    /** Players whose voice arrived in the last quarter second. */
    public java.util.List<UUID> talking() {
        java.util.List<UUID> out = new java.util.ArrayList<>();
        speakers.forEach((id, source) -> {
            if (source.isTalking()) out.add(id);
        });
        return out;
    }

    public void setTransmitting(boolean transmitting, boolean whispering) {
        this.transmitting = transmitting;
        this.whispering = whispering;
    }

    public boolean isMuted() {
        return muted;
    }

    public void setMuted(boolean muted) {
        this.muted = muted;
    }

    public boolean isConnected() {
        return connected;
    }

    public boolean isTransmitting() {
        return transmitting && !muted && connected;
    }

    @Override
    public void close() {
        running = false;
        send(new byte[] {0x04});
        speakers.values().forEach(SpeakerSource::close);
        speakers.clear();
        socket.close();
    }

    /** Opus decoder factory shared by speaker sources. */
    static OpusDecoder newDecoder() {
        try {
            OpusDecoder decoder = new OpusDecoder(SAMPLE_RATE, 1);
            decoder.setFrameSize(FRAME_SAMPLES);
            return decoder;
        } catch (Exception e) {
            throw new IllegalStateException("could not create the Opus decoder", e);
        }
    }
}
