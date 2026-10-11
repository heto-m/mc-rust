package dev.mcr.bridge.gpu;

import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** The backend's downcalls into mcr-modbridge. Arguments go through a per-thread block of longs. */
final class GpuNative {
    static final Logger LOGGER = LoggerFactory.getLogger("mcr-gpu");
    private static final int LOGGED_ERRORS = 50;
    private static final int ERROR_BYTES = 2048;

    private static final MethodHandle DEVICE = Bridge.get().gpuDevice;
    private static final MethodHandle ENCODE = Bridge.get().gpuEncode;
    private static final MethodHandle PASS = Bridge.get().gpuPass;
    private static final MethodHandle ERROR = Bridge.get().gpuError;
    private static final ThreadLocal<MemorySegment> ARGS =
            ThreadLocal.withInitial(() -> Arena.ofAuto().allocate(8L * BridgeAbi.GPU_MAX_ARGS, 8));
    private static final ThreadLocal<Scratch> SCRATCH = ThreadLocal.withInitial(Scratch::new);

    private static int errors;

    static MemorySegment args(long... values) {
        MemorySegment a = ARGS.get();
        for (int i = 0; i < values.length; i++) {
            a.setAtIndex(ValueLayout.JAVA_LONG, i, values[i]);
        }
        return a;
    }

    static long device(int op, long... values) {
        MemorySegment a = args(values);
        try {
            return (long) DEVICE.invokeExact(op, a.address(), values.length);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static long encode(long encoder, int op, long... values) {
        MemorySegment a = args(values);
        try {
            return (long) ENCODE.invokeExact(encoder, op, a.address(), values.length);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static long pass(long encoder, MemorySegment desc, int descCount, MemorySegment stream, int streamBytes) {
        try {
            return (long) PASS.invokeExact(encoder, desc.address(), descCount, stream.address(), streamBytes);
        } catch (Throwable t) {
            throw rethrow(t);
        }
    }

    static String lastError() {
        MemorySegment out = SCRATCH.get().get(ERROR_BYTES);
        int len;
        try {
            len = (int) ERROR.invokeExact(out.address(), ERROR_BYTES);
        } catch (Throwable t) {
            throw rethrow(t);
        }
        byte[] bytes = out.asSlice(0, Math.min(len, ERROR_BYTES)).toArray(ValueLayout.JAVA_BYTE);
        return new String(bytes, StandardCharsets.UTF_8);
    }

    /** Logs a failed call (rate-limited) and passes the result through. */
    static long check(long result, String what) {
        if (result < 0) {
            int n = ++errors;
            if (n <= LOGGED_ERRORS || n % 1000 == 0) {
                LOGGER.warn("{} failed ({} so far): {}", what, n, lastError());
            }
        }
        return result;
    }

    /** A handle or an exception: creation failures must not hand vanilla a dead object. */
    static long require(long result, String what) {
        if (result <= 0) {
            throw new IllegalStateException(what + " failed: " + lastError());
        }
        return result;
    }

    /** The native address of `data`'s remaining bytes, copying heap buffers into scratch memory. */
    static long address(ByteBuffer data) {
        if (data.isDirect()) {
            return MemorySegment.ofBuffer(data).address();
        }
        MemorySegment s = SCRATCH.get().get(data.remaining());
        MemorySegment.copy(MemorySegment.ofBuffer(data), 0, s, 0, data.remaining());
        return s.address();
    }

    static long handleOf(Object buffer) {
        if (buffer instanceof McrHandle h) {
            return h.handle();
        }
        throw new IllegalArgumentException("not an mcr GPU object: " + buffer);
    }

    static RuntimeException rethrow(Throwable t) {
        if (t instanceof RuntimeException r) {
            return r;
        }
        if (t instanceof Error e) {
            throw e;
        }
        return new IllegalStateException(t);
    }

    /** One growing native buffer per thread for heap-to-native copies. */
    private static final class Scratch {
        private MemorySegment segment = Arena.ofAuto().allocate(1 << 16, 16);

        MemorySegment get(long size) {
            if (segment.byteSize() < size) {
                segment = Arena.ofAuto().allocate(Math.max(size, segment.byteSize() * 2), 16);
            }
            return segment;
        }
    }

    private GpuNative() {}
}
