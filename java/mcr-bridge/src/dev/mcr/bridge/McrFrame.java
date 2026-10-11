package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import net.minecraft.client.Minecraft;

/** Paces the render thread on Rust's frame messages. Only the render thread calls in here. */
public final class McrFrame {
    private static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG_UNALIGNED;
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfFloat FLOAT = ValueLayout.JAVA_FLOAT_UNALIGNED;

    private static boolean stopping;
    private static long frameIndex = -1;
    private static int ticksToDo;
    private static int width;
    private static int height;
    private static float partialTick;
    private static float scale = 1.0F;

    public static long frameIndex() {
        return frameIndex;
    }

    public static int ticksToDo() {
        return ticksToDo;
    }

    /** Blocks in native until a frame or a shutdown arrives; queued frames collapse into the newest. */
    public static void awaitFrame(Minecraft minecraft) {
        if (stopping) {
            return;
        }
        Bridge bridge = Bridge.get();
        while (true) {
            int seen = bridge.controlBell.seq();
            if (drain(bridge, minecraft)) {
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
                        frameIndex = payload.get(LONG, BridgeAbi.FRAME_INDEX);
                        ticks += payload.get(INT, BridgeAbi.FRAME_TICKS);
                        width = payload.get(INT, BridgeAbi.FRAME_WIDTH);
                        height = payload.get(INT, BridgeAbi.FRAME_HEIGHT);
                        partialTick = payload.get(FLOAT, BridgeAbi.FRAME_PARTIAL_TICK);
                        scale = payload.get(FLOAT, BridgeAbi.FRAME_SCALE);
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
        if (stopping) {
            Bridge.report(BridgeAbi.REPORT_STOPPING, "shutdown requested");
            minecraft.stop();
        }
        return woke;
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
