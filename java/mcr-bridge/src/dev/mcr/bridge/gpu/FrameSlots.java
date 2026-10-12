package dev.mcr.bridge.gpu;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrFrame;

/** Tags Java's GPU work for mcr's compositor: frame work is immediate, `GuiRenderer.render` is the GUI slot. */
public final class FrameSlots {
    private static long frame;
    private static boolean gui;
    private static int mainTarget;

    public static void beginFrame(long index) {
        frame = index;
        gui = false;
        slot(BridgeAbi.SLOT_IMMEDIATE);
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
