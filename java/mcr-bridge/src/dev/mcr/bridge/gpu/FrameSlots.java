package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrFrame;
import java.util.ArrayList;
import java.util.List;
import org.joml.Matrix4fc;

/**
 * Tags Java's GPU work for mcr's compositor. Frame work is immediate; `LevelRenderer.render` runs in
 * the world slots, cut at vanilla's phase boundaries; the 3D HUD and post effects are `POST_WORLD`;
 * `GuiRenderer.render` is the GUI slot.
 */
public final class FrameSlots {
    private static long frame;
    private static boolean gui;
    private static int current = BridgeAbi.SLOT_IMMEDIATE;
    private static int mainTarget;
    private static boolean guiDone;
    private static final int MAX_DEFERRED = 16;
    private static final List<Runnable> afterGui = new ArrayList<>();
    private static final long[] VIEW_ARGS = new long[22];

    public static void beginFrame(long index) {
        runDeferred();
        frame = index;
        gui = false;
        guiDone = false;
        slot(BridgeAbi.SLOT_IMMEDIATE);
    }

    /**
     * Java's main color is mcr's scene, which holds this frame's partial world until the GUI slot ends;
     * a read of it before then waits until after the GUI, when the scene is the whole frame.
     */
    public static boolean deferMainRead(Runnable read) {
        if (!McrFrame.lockstep() || guiDone || current != BridgeAbi.SLOT_IMMEDIATE || afterGui.size() >= MAX_DEFERRED) {
            return false;
        }
        afterGui.add(read);
        return true;
    }

    /** Runs the reads deferred to the end of the frame's GUI, or of the frame when it had none. */
    public static void runDeferred() {
        guiDone = true;
        if (afterGui.isEmpty()) {
            return;
        }
        List<Runnable> reads = new ArrayList<>(afterGui);
        afterGui.clear();
        for (Runnable read : reads) {
            read.run();
        }
    }

    /** `LevelRenderer.render` starts: Java's main target is mcr's scene until the GUI. */
    public static void beginWorld() {
        if (!McrFrame.lockstep() || gui || current != BridgeAbi.SLOT_IMMEDIATE) {
            return;
        }
        split();
        slot(BridgeAbi.SLOT_WORLD_SOLID);
    }

    /**
     * Ends the current world slot at a vanilla phase boundary; later slots never go back. Slots mcr submits at
     * the same point share a command buffer unless a pass is open across the boundary.
     */
    public static void cutTo(int next) {
        if (!inWorld() || next <= current) {
            return;
        }
        if (drainPoint(next) != drainPoint(current) || passOpen()) {
            split();
        }
        slot(next);
    }

    /** Where mcr submits a slot's work: before its translucent terrain, after its world, or in the GUI. */
    static int drainPoint(int slot) {
        return switch (slot) {
            case BridgeAbi.SLOT_WORLD_SOLID, BridgeAbi.SLOT_WORLD_TRANSLUCENT -> 1;
            case BridgeAbi.SLOT_WORLD_AFTER, BridgeAbi.SLOT_POST_WORLD -> 2;
            case BridgeAbi.SLOT_GUI -> 3;
            default -> 0;
        };
    }

    private static boolean passOpen() {
        McrDeviceBackend device = McrDeviceBackend.instance;
        return device != null && device.encoder().inPass();
    }

    /** Java draws into mcr's scene with mcr's top-down projection. */
    public static boolean inWorld() {
        return current >= BridgeAbi.SLOT_WORLD_SOLID && current <= BridgeAbi.SLOT_POST_WORLD;
    }

    public static void beginGui() {
        if (!McrFrame.lockstep() || gui) {
            return;
        }
        split();
        gui = true;
        slot(BridgeAbi.SLOT_GUI);
    }

    public static void endGui() {
        if (!gui) {
            return;
        }
        split();
        gui = false;
        slot(BridgeAbi.SLOT_IMMEDIATE);
        runDeferred();
    }

    private static void split() {
        McrDeviceBackend device = McrDeviceBackend.instance;
        if (device != null) {
            device.encoder().split();
        }
    }

    private static void slot(int slot) {
        current = slot;
        GpuNative.check(GpuNative.device(BridgeAbi.D_SLOT, slot, frame), "frame slot");
    }

    /**
     * The level projection the world slots read this frame, with the camera Java's geometry is relative
     * to, so mcr can redraw a frame it composes late from its current camera.
     */
    public static void levelView(GpuBufferSlice projection, double x, double y, double z, Matrix4fc viewRotation) {
        if (!McrFrame.lockstep() || !(projection.buffer() instanceof McrHandle buffer)) {
            return;
        }
        long[] a = VIEW_ARGS;
        a[0] = frame;
        a[1] = buffer.handle();
        a[2] = projection.offset();
        a[3] = Double.doubleToRawLongBits(x);
        a[4] = Double.doubleToRawLongBits(y);
        a[5] = Double.doubleToRawLongBits(z);
        // The inverse of a rotation is its transpose.
        for (int col = 0; col < 4; col++) {
            for (int row = 0; row < 4; row++) {
                a[6 + col * 4 + row] = Float.floatToRawIntBits(viewRotation.get(row, col)) & 0xFFFFFFFFL;
            }
        }
        GpuNative.check(GpuNative.device(BridgeAbi.D_LEVEL_VIEW, a), "level view");
    }

    /** `MainTarget` is allocating its attachments: they become the compositor's virtual targets. */
    public static void enterMainTarget() {
        mainTarget++;
    }

    public static void exitMainTarget() {
        mainTarget = Math.max(0, mainTarget - 1);
    }

    static int textureUsage(int usage) {
        return McrFrame.COMPOSE && mainTarget > 0 ? usage | BridgeAbi.GPU_TEXTURE_VIRTUAL : usage;
    }

    private FrameSlots() {}
}
