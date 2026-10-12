package dev.mcr.bridge;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.platform.Window;
import dev.mcr.bridge.gpu.FrameSlots;
import dev.mcr.bridge.mixin.WindowAccessor;
import dev.mcr.bridge.net.McrNet;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;
import net.minecraft.client.Minecraft;
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

    public static float partialTick() {
        return partialTick;
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
            publish(bridge, minecraft, BridgeAbi.FSTATE_READY);
        }
        while (true) {
            int seen = bridge.controlBell.seq();
            if (drain(bridge, minecraft)) {
                if (!stopping) {
                    frameStart = System.nanoTime();
                    if (lockstep()) {
                        FrameSlots.beginFrame(frameIndex);
                        applySize(minecraft);
                    }
                    McrNet.pump(minecraft);
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
        boolean loaded = minecraft.isGameLoadFinished() && minecraft.gui.overlay() == null;
        publish(Bridge.get(), minecraft, BridgeAbi.FSTATE_READY | (loaded ? BridgeAbi.FSTATE_LOADED : 0));
    }

    private static void publish(Bridge bridge, Minecraft minecraft, int stateFlags) {
        MemorySegment state = bridge.frameState;
        RenderTarget main = minecraft.gameRenderer == null ? null : minecraft.gameRenderer.mainRenderTarget();
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_TICKS, clientTicks);
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_NANOS, frameStart == 0 ? 0L : System.nanoTime() - frameStart);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_FLAGS, stateFlags);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_WIDTH, main == null ? 0 : main.width);
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_HEIGHT, main == null ? 0 : main.height);
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
