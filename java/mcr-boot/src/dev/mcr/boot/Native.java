package dev.mcr.boot;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

public final class Native {
    private static volatile MemorySegment table;

    static void init(long address) {
        if (table != null) {
            throw new IllegalStateException("native table already initialised");
        }
        MemorySegment header = MemorySegment.ofAddress(address).reinterpret(Abi.TABLE_HEADER_SIZE);
        int magic = header.get(ValueLayout.JAVA_INT, 0);
        int version = header.get(ValueLayout.JAVA_INT, 4);
        int size = header.get(ValueLayout.JAVA_INT, 8);
        int count = header.get(ValueLayout.JAVA_INT, 12);
        checkHeader(magic, version, size, count, Abi.VERSION);
        table = MemorySegment.ofAddress(address).reinterpret(size);
        Upcalls.register();
    }

    public static void checkHeader(int magic, int version, int size, int count, int expectedVersion) {
        if (magic != Abi.MAGIC) {
            throw new IllegalStateException(String.format("native table magic 0x%08x, expected 0x%08x", magic, Abi.MAGIC));
        }
        if (version != expectedVersion) {
            throw new IllegalStateException(
                    "native table version " + version + " does not match boot code version " + expectedVersion);
        }
        int entries = Abi.Entry.values().length;
        if (count < entries || size < Abi.TABLE_HEADER_SIZE + 8L * count) {
            throw new IllegalStateException(
                    "native table has " + count + " entries in " + size + " bytes, boot code needs " + entries);
        }
    }

    private static MethodHandle downcall(Abi.Entry entry) {
        MemorySegment t = table;
        if (t == null) {
            throw new IllegalStateException("native table not initialised");
        }
        long fn = t.get(ValueLayout.JAVA_LONG, Abi.TABLE_HEADER_SIZE + 8L * entry.ordinal());
        FunctionDescriptor fd = Descriptors.parse(entry.descriptor);
        Linker.Option[] options = entry.flavour == Abi.Flavour.CRITICAL
                ? new Linker.Option[] {Linker.Option.critical(false)}
                : new Linker.Option[0];
        return Linker.nativeLinker().downcallHandle(MemorySegment.ofAddress(fn), fd, options);
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
