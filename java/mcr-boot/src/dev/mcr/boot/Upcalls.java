package dev.mcr.boot;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;

/** An exception escaping an upcall kills the VM, so every target catches everything and returns a status. */
public final class Upcalls {
    public static final int OK = 0;
    public static final int FAILED = -1;

    private static volatile Throwable lastFailure;

    static void register() {
        Abi.Upcall[] all = Abi.Upcall.values();
        Linker linker = Linker.nativeLinker();
        MethodHandles.Lookup lookup = MethodHandles.lookup();
        MemorySegment stubs = Arena.global().allocate(ValueLayout.JAVA_LONG, all.length);
        for (Abi.Upcall upcall : all) {
            FunctionDescriptor fd = Descriptors.parse(upcall.descriptor);
            MethodHandle target;
            try {
                target = lookup.findStatic(Upcalls.class, Descriptors.camelCase(upcall.symbol), fd.toMethodType());
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException("missing upcall target " + upcall.symbol, e);
            }
            MemorySegment stub = linker.upcallStub(target, fd, Arena.global());
            stubs.setAtIndex(ValueLayout.JAVA_LONG, upcall.ordinal(), stub.address());
        }
        int status = Native.registerUpcalls(stubs.address(), all.length);
        if (status != OK) {
            throw new IllegalStateException("register_upcalls failed with status " + status);
        }
    }

    public static Throwable lastFailure() {
        return lastFailure;
    }

    private static int failed(Throwable t) {
        lastFailure = t;
        return FAILED;
    }

    static int selftestPing() {
        try {
            return OK;
        } catch (Throwable t) {
            return failed(t);
        }
    }

    static int selftestAdd(long address, long delta) {
        try {
            MemorySegment counter = MemorySegment.ofAddress(address).reinterpret(8);
            counter.set(ValueLayout.JAVA_LONG, 0, counter.get(ValueLayout.JAVA_LONG, 0) + delta);
            return OK;
        } catch (Throwable t) {
            return failed(t);
        }
    }

    static int selftestThrow() {
        try {
            throw new IllegalStateException("selftest upcall failure");
        } catch (Throwable t) {
            return failed(t);
        }
    }

    private Upcalls() {}
}
