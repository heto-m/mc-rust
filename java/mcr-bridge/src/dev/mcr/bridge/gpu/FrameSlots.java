package dev.mcr.bridge.gpu;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrFrame;

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

    public static void beginFrame(long index) {
        frame = index;
        gui = false;
        slot(BridgeAbi.SLOT_IMMEDIATE);
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
