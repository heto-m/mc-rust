package dev.mcr.bridge;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.platform.Window;
import dev.mcr.bridge.gpu.FrameSlots;
import dev.mcr.bridge.mixin.LoadingOverlayAccessor;
import dev.mcr.bridge.mixin.WindowAccessor;
import dev.mcr.bridge.net.McrNet;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.GenericMessageScreen;
import net.minecraft.client.gui.screens.LoadingOverlay;
import net.minecraft.client.gui.screens.Overlay;
import net.minecraft.client.gui.screens.LevelLoadingScreen;
import net.minecraft.client.gui.screens.ProgressScreen;
import net.minecraft.client.gui.screens.Screen;
import org.lwjgl.sdl.SDLVideo;

/** Paces the render thread on Rust's frame messages. Only the render thread calls in here. */
public final class McrFrame {
    /** Rust composes Java's GUI into its own frame and decides every tick (`-Dmcr.compose`). */
    public static final boolean COMPOSE = Boolean.getBoolean(BridgeAbi.COMPOSE_PROPERTY);

    private static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG_UNALIGNED;
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfFloat FLOAT = ValueLayout.JAVA_FLOAT_UNALIGNED;
    private static final VarHandle STATE_LONG = ValueLayout.JAVA_LONG.varHandle();
    private static final VarHandle STATE_INT = ValueLayout.JAVA_INT.varHandle();
    private static final int MAX_TICKS_PER_FRAME = 10;
    private static final long MAX_PENDING_TICKS = 1 << 20;
    private static final int MAX_SIZE = 16384;

    private static boolean stopping;
    private static long frameIndex = -1;
    private static int ticksToDo;
    private static long pendingTicks;
    private static int width;
    private static int height;
    private static float partialTick;
    private static int flags;
    private static long clientTicks;
    private static long frameStart;
    private static boolean announced;
    private static final McrCamera camera = new McrCamera();
    private static final McrCamera renderCamera = new McrCamera();

    public static long frameIndex() {
        return frameIndex;
    }

    public static int ticksToDo() {
        return ticksToDo;
    }

    /** The current frame came from the render thread's lockstep: ticks, partial tick and slots are Rust's. */
    public static boolean lockstep() {
        return COMPOSE && (flags & BridgeAbi.FRAME_LOCKSTEP) != 0;
    }

    /** Rust asked Java to shut down; Java's own Quit is a request to Rust instead. */
    public static boolean stopping() {
        return stopping;
    }

    public static float partialTick() {
        return partialTick;
    }

    /** {@code Camera.update} sets it: Java's field of view over vanilla's, i.e. what FOV mods changed. */
    public static volatile float fovScale = 1.0F;

    /** mcr's camera for this frame, or null outside the lockstep or when Rust's values are unusable. */
    public static @org.jspecify.annotations.Nullable McrCamera camera() {
        return lockstep() && camera.valid() ? camera : null;
    }

    /** The mod camera mcr renders this frame instead of {@link #camera()}, or null when it renders that. */
    public static @org.jspecify.annotations.Nullable McrCamera renderCamera() {
        return camera() != null && renderCamera.valid() ? renderCamera : null;
    }

    /** Ticks this frame runs; anything over vanilla's cap carries to the next frames instead of being lost. */
    public static int takeTicks() {
        int run = (int) Math.min(pendingTicks, MAX_TICKS_PER_FRAME);
        pendingTicks -= run;
        return run;
    }

    public static void tickRan() {
        clientTicks++;
    }

    /** Blocks in native until a frame or a shutdown arrives; queued frames collapse into the newest. */
    public static void awaitFrame(Minecraft minecraft) {
        if (stopping) {
            return;
        }
        Bridge bridge = Bridge.get();
        McrNet.init(Thread.currentThread());
        if (COMPOSE && !announced) {
            announced = true;
            McrProfiler.start(Thread.currentThread());
            publish(bridge, minecraft, BridgeAbi.FSTATE_READY);
        }
        while (true) {
            int seen = bridge.controlBell.seq();
            if (drain(bridge, minecraft)) {
                if (!stopping) {
                    frameStart = System.nanoTime();
                    McrFrameTimes.reset();
                    McrFrameTimes.begin(BridgeAbi.PROF_PRE);
                    McrParticles.reset();
                    McrViewRoll.reset();
                    if (lockstep()) {
                        FrameSlots.beginFrame(frameIndex);
                        applySize(minecraft);
                        McrSync.applyEntityScale(minecraft);
                    }
                    McrWorlds.pump(minecraft);
                    McrNet.pump(minecraft);
                    if (lockstep()) {
                        McrInput.deliver(minecraft);
                    }
                    McrFrameTimes.end(BridgeAbi.PROF_PRE);
                }
                return;
            }
            bridge.controlBell.await(seen, -1);
        }
    }

    private static boolean drain(Bridge bridge, Minecraft minecraft) {
        boolean woke = false;
        int ticks = 0;
        int kind;
        while ((kind = bridge.control.poll()) >= 0) {
            MemorySegment payload = bridge.control.payload();
            switch (kind) {
                case BridgeAbi.MSG_FRAME -> {
                    if (payload.byteSize() >= BridgeAbi.FRAME_SIZE) {
                        read(payload);
                        ticks = (int) Math.min((long) ticks + Math.max(0, payload.get(INT, BridgeAbi.FRAME_TICKS)), MAX_PENDING_TICKS);
                    }
                    woke = true;
                }
                case BridgeAbi.MSG_INPUT -> McrInput.enqueue(payload);
                case BridgeAbi.MSG_KEYS -> McrSync.applyKeys(payload);
                case BridgeAbi.MSG_PACKS -> McrSync.applyPacks(minecraft, payload);
                case BridgeAbi.MSG_WORLD -> McrWorlds.receive(payload);
                case BridgeAbi.MSG_MODELS -> McrRenderExport.request(payload);
                case BridgeAbi.MSG_SHUTDOWN -> {
                    stopping = true;
                    woke = true;
                }
                default -> {}
            }
            bridge.control.release();
        }
        ticksToDo = ticks;
        if (lockstep()) {
            pendingTicks = Math.min(pendingTicks + ticks, MAX_PENDING_TICKS);
        }
        if (stopping) {
            Bridge.report(BridgeAbi.REPORT_STOPPING, "shutdown requested");
            minecraft.stop();
        }
        return woke;
    }

    private static void read(MemorySegment p) {
        frameIndex = p.get(LONG, BridgeAbi.FRAME_INDEX);
        width = p.get(INT, BridgeAbi.FRAME_WIDTH);
        height = p.get(INT, BridgeAbi.FRAME_HEIGHT);
        float partial = p.get(FLOAT, BridgeAbi.FRAME_PARTIAL_TICK);
        partialTick = Float.isFinite(partial) ? Math.clamp(partial, 0.0F, 1.0F) : 0.0F;
        flags = p.get(INT, BridgeAbi.FRAME_FLAGS);
        camera.read(p, (flags & BridgeAbi.FRAME_DETACHED) != 0);
        renderCamera.readRender(p, (flags & BridgeAbi.FRAME_CAMERA_HANDOFF) != 0);
        McrSections.frame(Bridge.get().sections, (flags & BridgeAbi.FRAME_SECTIONS) != 0);
        McrSync.readView(p);
    }

    /** The mcr window's size reaches `Window` and the main target before this frame renders. */
    private static void applySize(Minecraft minecraft) {
        if (width < 1 || height < 1 || width > MAX_SIZE || height > MAX_SIZE) {
            return;
        }
        Window window = minecraft.getWindow();
        if (window.getWidth() == width && window.getHeight() == height) {
            return;
        }
        SDLVideo.SDL_SetWindowSize(window.handle(), width, height);
        WindowAccessor access = (WindowAccessor) (Object) window;
        access.mcr$setScreenWidth(width);
        access.mcr$setScreenHeight(height);
        window.setWidth(width);
        window.setHeight(height);
        minecraft.framebufferSizeChanged();
    }

    /** Ends the lockstep frame: Rust may now submit its GPU work and post the next one. */
    public static void frameDone(Minecraft minecraft) {
        if (!lockstep() || frameIndex < 0) {
            return;
        }
        FrameSlots.runDeferred();
        boolean loaded = resourcesLoaded(minecraft);
        int waiting = waitingScreen(minecraft.gui.screen()) ? BridgeAbi.FSTATE_WAITING : 0;
        McrInput.publish(minecraft);
        int weather = McrWeather.takeReplaced() ? BridgeAbi.FSTATE_WEATHER : 0;
        int steps = McrSteps.replaced() ? BridgeAbi.FSTATE_STEPS : 0;
        int turn = McrTurn.captured() ? BridgeAbi.FSTATE_TURN : 0;
        McrDynamicLights.publish(minecraft);
        McrRenderExport.frame(minecraft);
        publish(Bridge.get(), minecraft, BridgeAbi.FSTATE_READY | (loaded ? BridgeAbi.FSTATE_LOADED : 0) | waiting | weather | steps | turn);
    }

    /** Loading finished and no reload runs; a loading overlay may still be fading out, as vanilla's quick play connects then. */
    public static boolean resourcesLoaded(Minecraft minecraft) {
        if (!minecraft.isGameLoadFinished()) {
            return false;
        }
        Overlay overlay = minecraft.gui.overlay();
        return overlay == null || overlay instanceof LoadingOverlay loading && ((LoadingOverlayAccessor) loading).mcr$fadeOutStart() != -1L;
    }

    private static boolean waitingScreen(Screen screen) {
        return screen instanceof LevelLoadingScreen || screen instanceof ConnectScreen || screen instanceof GenericMessageScreen
            || screen instanceof ProgressScreen || screen instanceof McrWorldScreen;
    }

    private static void publish(Bridge bridge, Minecraft minecraft, int stateFlags) {
        MemorySegment state = bridge.frameState;
        RenderTarget main = minecraft.gameRenderer == null ? null : minecraft.gameRenderer.mainRenderTarget();
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_TICKS, clientTicks);
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_NANOS, frameStart == 0 ? 0L : System.nanoTime() - frameStart);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_FLAGS, stateFlags);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_WIDTH, main == null ? 0 : main.width);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_HEIGHT, main == null ? 0 : main.height);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_FOV_SCALE, Float.floatToRawIntBits(fovScale));
        McrCameraHandoff.publish(state, minecraft);
        McrFrameTimes.publish(state);
        if (frameIndex >= 0 && lockstep()) {
            STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_DONE, frameIndex + 1);
        }
        bridge.frameBell.ring();
    }

    /** After a crash the game cannot continue; the thread sleeps in native until the process exits. */
    public static void parkForever() {
        Bridge bridge = Bridge.get();
        while (true) {
            int seen = bridge.controlBell.seq();
            while (bridge.control.poll() >= 0) {
                bridge.control.release();
            }
            bridge.controlBell.await(seen, -1);
        }
    }

    private McrFrame() {}
}
