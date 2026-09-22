package org.garnetmc.client.render;

import com.mojang.blaze3d.buffers.Std140Builder;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.RenderPass;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.MappableRingBuffer;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import net.minecraft.client.renderer.state.level.SkyRenderState;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.material.FogType;
import org.garnetmc.client.GarnetClient;
import org.garnetmc.client.mixin.LevelRendererAccessor;
import org.joml.Matrix4f;
import org.joml.Vector3f;

/**
 * Garnet Render: our own shader pipeline.
 *
 * Minecraft 26 renders through its own GPU abstraction (OpenGL or Vulkan,
 * the player's choice) and runs a post-effect chain at the end of every
 * frame. We point that hook at {@code garnet:render}, a chain of full-screen
 * passes that read the finished frame plus its depth buffer and rebuild the
 * look: ambient occlusion, screen-space sun shadows, atmosphere and light
 * shafts, bloom, filmic tone mapping. Because it sits on the vanilla
 * abstraction it works on both backends and next to any other mod.
 *
 * The passes need per-frame data vanilla does not give post effects
 * (matrices, sun direction, fog, our settings). {@link #bindFrame} writes
 * that into a small uniform block right before each of our passes draws.
 */
public final class GarnetRender {
    public static final String NAMESPACE = "garnet";
    public static final Identifier PIPELINE = Identifier.fromNamespaceAndPath(NAMESPACE, "render");
    private static final Identifier VANILLA_END_OF_FRAME = Identifier.withDefaultNamespace("end_of_frame");
    private static final int BLOCK_SIZE = 512;

    public static final RenderSettings settings = RenderSettings.load();
    private static boolean enabled = settings.enabled;
    private static MappableRingBuffer frameBlock;
    private static final Matrix4f scratch = new Matrix4f();
    private static final Vector3f sunWorld = new Vector3f();
    private static final Vector3f sunView = new Vector3f();

    private GarnetRender() {}

    public static void init() {
        // The GPU device only exists once the window is up, so the terrain
        // map texture is created when the client has finished starting.
        ClientLifecycleEvents.CLIENT_STARTED.register(TerrainMap::register);
        apply();
    }

    private static final boolean DEBUG = Boolean.getBoolean("garnet.debug");
    private static int debugTicks;

    /** Client tick. */
    public static void tick(Minecraft mc) {
        TerrainMap.tick(mc);
        if (DEBUG && ++debugTicks % 100 == 0 && mc.level != null) {
            log("fps " + mc.getFps() + (enabled ? " with" : " without") + " Garnet Render");
        }
    }

    public static boolean isEnabled() {
        return enabled;
    }

    public static void setEnabled(boolean on) {
        enabled = on;
        settings.enabled = on;
        settings.save();
        apply();
    }

    public static void toggle() {
        setEnabled(!enabled);
    }

    private static void apply() {
        GameRenderer.END_OF_FRAME_POST_EFFECT = enabled ? PIPELINE : VANILLA_END_OF_FRAME;
    }

    private static GpuBuffer pending;

    /** Fills the GarnetFrame block once per frame, before our chain runs (no render pass may be open). */
    public static void writeFrame() {
        pending = null;
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null) return;
        TerrainMap.upload();
        LevelRenderState state = ((LevelRendererAccessor) mc.levelRenderer).garnet$levelRenderState();
        CameraRenderState cam = state.cameraRenderState;
        if (cam == null || cam.projectionMatrix == null || cam.viewRotationMatrix == null) return;

        if (frameBlock == null) {
            frameBlock = new MappableRingBuffer(() -> "Garnet frame block", GpuBuffer.USAGE_UNIFORM | GpuBuffer.USAGE_MAP_WRITE, BLOCK_SIZE);
        }
        GpuBuffer buffer = frameBlock.currentBuffer();
        try (GpuBufferSlice.MappedView view = buffer.map(false, true)) {
            write(Std140Builder.intoBuffer(view.data()), mc, state, cam);
        }
        frameBlock.rotate();
        pending = buffer;
    }

    /** Binds the block written by {@link #writeFrame} to the pass that is drawing. */
    public static void bindFrame(RenderPass pass) {
        if (pending != null) {
            pass.setUniform("GarnetFrame", pending);
        }
    }

    private static void write(Std140Builder b, Minecraft mc, LevelRenderState state, CameraRenderState cam) {
        SkyRenderState sky = state.skyRenderState;
        float partial = state.worldPartialTicks;

        // The sky renderer turns the sun around X by its angle, then the
        // whole sky by -90 around Y, with the sun quad pointing up; that puts
        // the sun at (-sin a, cos a, 0): east at sunrise, overhead at noon.
        float a = sky.sunAngle;
        sunWorld.set(-Math.sin(a), Math.cos(a), 0.0).normalize();
        cam.viewRotationMatrix.transformDirection(sunWorld, sunView).normalize();
        float daylight = smoothstep(-0.10f, 0.15f, sunWorld.y);

        double time = (state.gameTime + partial) / 20.0;
        float rain = mc.level.getRainLevel(partial);
        boolean underwater = cam.fogType == FogType.WATER;
        boolean zeroToOne = RenderSystem.getDevice().getDeviceInfo().isZZeroToOne();
        float fogEnd = cam.fogData != null ? cam.fogData.renderDistanceEnd : 256f;
        float envEnd = cam.fogData != null ? cam.fogData.environmentalEnd : fogEnd;

        b.putMat4f(cam.projectionMatrix);
        b.putMat4f(scratch.set(cam.projectionMatrix).invert());
        b.putMat4f(cam.viewRotationMatrix);
        b.putMat4f(scratch.set(cam.viewRotationMatrix).invert());
        b.putVec4(sunView.x, sunView.y, sunView.z, daylight);
        b.putVec4(sunWorld.x, sunWorld.y, sunWorld.z, sunWorld.y);
        // Camera relative to the terrain map's corner keeps the shader maths small.
        b.putVec4((float) (cam.pos.x - TerrainMap.originX()), (float) cam.pos.y, (float) (cam.pos.z - TerrainMap.originZ()), (float) (time % 3600.0));
        b.putVec4(envEnd, fogEnd, rain, underwater ? 1f : 0f);
        // The sky state is only filled in for dimensions with a sky; fall back to a plain blue.
        float skyR = 0.48f, skyG = 0.65f, skyB = 1.0f;
        if (sky.skyColor != null) {
            skyR = sky.skyColor.x();
            skyG = sky.skyColor.y();
            skyB = sky.skyColor.z();
        }
        b.putVec4(skyR, skyG, skyB, sky.sunriseAndSunsetColor != null ? sky.sunriseAndSunsetColor.w() : 0f);
        b.putVec4(settings.ambientOcclusion, settings.shadows, settings.bloom, settings.exposure);
        b.putVec4(0.05f, cam.depthFar, zeroToOne ? 1f : 0f, settings.lightShafts);
        b.putVec4(TerrainMap.SIZE, 1f / TerrainMap.SIZE, TerrainMap.isReady() ? 1f : 0f, settings.water);
    }

    private static float smoothstep(float edge0, float edge1, float x) {
        float t = Math.max(0f, Math.min(1f, (x - edge0) / (edge1 - edge0)));
        return t * t * (3f - 2f * t);
    }

    public static void log(String message) {
        GarnetClient.LOG.info("[render] {}", message);
    }
}
