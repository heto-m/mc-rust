package dev.mcr.bridge.gpu;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrFrame;
import java.util.ArrayList;
import java.util.List;

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

    /** Ends the current world slot at a vanilla phase boundary; later slots never go back. */
    public static void cutTo(int next) {
        if (!inWorld() || next <= current) {
            return;
        }
        split();
        slot(next);
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
