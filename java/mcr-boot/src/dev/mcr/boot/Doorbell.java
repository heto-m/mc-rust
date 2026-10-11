package dev.mcr.boot;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;

/** Read seq, re-check the condition, then await on the seen value. */
public final class Doorbell {
    private static final VarHandle INT = ValueLayout.JAVA_INT.varHandle();

    private final long address;
    private final MemorySegment word;

    public Doorbell(long address) {
        this.address = address;
        this.word = MemorySegment.ofAddress(address).reinterpret(4);
    }

    public int seq() {
        return (int) INT.getAcquire(word, 0L);
    }

    /** Waits in native code, so the thread never holds up a safepoint. Negative timeout waits forever. */
    public boolean await(int seen, long timeoutNanos) {
        return Native.doorbellWait(address, seen, timeoutNanos) != 0;
    }

    public void ring() {
        Native.doorbellNotify(address);
    }
}
