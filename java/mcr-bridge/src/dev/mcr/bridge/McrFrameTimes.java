package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;
import java.util.Arrays;

/** Phase times of the current Java frame, published with it for mcr's frame profile. Render thread only. */
public final class McrFrameTimes {
    private static final VarHandle STATE_INT = ValueLayout.JAVA_INT.varHandle();
    private static final long[] start = new long[BridgeAbi.PROF_COUNT];
    private static final long[] total = new long[BridgeAbi.PROF_COUNT];

    public static void reset() {
        Arrays.fill(start, 0L);
        Arrays.fill(total, 0L);
    }

    public static void begin(int phase) {
        start[phase] = System.nanoTime();
    }

    public static void end(int phase) {
        long began = start[phase];
        if (began != 0L) {
            total[phase] += System.nanoTime() - began;
            start[phase] = 0L;
        }
    }

    static void publish(MemorySegment state) {
        for (int i = 0; i < BridgeAbi.PROF_COUNT; i++) {
            int micros = (int) Math.min(total[i] / 1000L, Integer.MAX_VALUE);
            STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_PROF + 4L * i, micros);
        }
    }

    private McrFrameTimes() {}
}
