package dev.mcr.boot;

import java.lang.invoke.MethodHandle;

public final class Native {
    private static volatile NativeTable table;

    static void init(long address) {
        if (table != null) {
            throw new IllegalStateException("native table already initialised");
        }
        table = NativeTable.map(address, Abi.MAGIC, Abi.VERSION, Abi.Entry.values().length);
        Upcalls.register();
    }

    public static boolean initialised() {
        return table != null;
    }

    public static void checkHeader(int magic, int version, int size, int count, int expectedVersion) {
        NativeTable.check(magic, version, size, count, Abi.MAGIC, expectedVersion, Abi.Entry.values().length);
    }

    private static MethodHandle downcall(Abi.Entry entry) {
        NativeTable t = table;
        if (t == null) {
            throw new IllegalStateException("native table not initialised");
        }
        return t.downcall(entry.ordinal(), entry.descriptor, entry.flavour == Abi.Flavour.CRITICAL);
    }

    // Static finals let the JIT treat the handles as constants; the holder initialises after init().
    private static final class H {
        static final MethodHandle REGISTER_UPCALLS = downcall(Abi.Entry.REGISTER_UPCALLS);
        static final MethodHandle HOST_CALL = downcall(Abi.Entry.HOST_CALL);
        static final MethodHandle DOORBELL_WAIT = downcall(Abi.Entry.DOORBELL_WAIT);
        static final MethodHandle DOORBELL_NOTIFY = downcall(Abi.Entry.DOORBELL_NOTIFY);
        static final MethodHandle NOP_CRITICAL = downcall(Abi.Entry.NOP_CRITICAL);
        static final MethodHandle NOP_NORMAL = downcall(Abi.Entry.NOP_NORMAL);
    }

    static int registerUpcalls(long stubs, int count) {
        try {
            return (int) H.REGISTER_UPCALLS.invokeExact(stubs, count);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    public static long hostCall(int op, long arg) {
        try {
            return (long) H.HOST_CALL.invokeExact(op, arg);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    public static int doorbellWait(long bell, int seen, long timeoutNanos) {
        try {
            return (int) H.DOORBELL_WAIT.invokeExact(bell, seen, timeoutNanos);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    public static void doorbellNotify(long bell) {
        try {
            H.DOORBELL_NOTIFY.invokeExact(bell);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    public static long nopCritical(long x) {
        try {
            return (long) H.NOP_CRITICAL.invokeExact(x);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    public static long nopNormal(long x) {
        try {
            return (long) H.NOP_NORMAL.invokeExact(x);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    private static RuntimeException rethrow(Throwable t) {
        if (t instanceof RuntimeException r) {
            return r;
        }
        if (t instanceof Error e) {
            throw e;
        }
        return new IllegalStateException(t);
    }

    private Native() {}
}
