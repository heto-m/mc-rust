package dev.mcr.bridge;

import dev.mcr.boot.Abi;
import dev.mcr.boot.Doorbell;
import dev.mcr.boot.NativeTable;
import dev.mcr.boot.Ring;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;

/** The bridge's view of mcr: its extension table, the control and net rings, and the report downcall. */
public final class Bridge {
    private static final int MAX_REPORT_BYTES = 16 << 10;

    private static Bridge instance;

    private final MethodHandle sharedAddress;
    private final MethodHandle report;
    public final MethodHandle gpuDevice;
    public final MethodHandle gpuEncode;
    public final MethodHandle gpuPass;
    public final MethodHandle gpuError;
    final Ring control;
    final Doorbell controlBell;
    public final Ring netIn;
    public final Ring netOut;

    private Bridge(NativeTable table) {
        this.sharedAddress = downcall(table, BridgeAbi.Entry.SHARED_ADDRESS);
        this.report = downcall(table, BridgeAbi.Entry.REPORT);
        this.gpuDevice = downcall(table, BridgeAbi.Entry.GPU_DEVICE);
        this.gpuEncode = downcall(table, BridgeAbi.Entry.GPU_ENCODE);
        this.gpuPass = downcall(table, BridgeAbi.Entry.GPU_PASS);
        this.gpuError = downcall(table, BridgeAbi.Entry.GPU_ERROR);
        this.control = Ring.at(shared(BridgeAbi.SHARED_CONTROL_RING));
        this.controlBell = new Doorbell(shared(BridgeAbi.SHARED_CONTROL_BELL));
        this.netIn = Ring.at(shared(BridgeAbi.SHARED_NET_IN_RING));
        this.netOut = Ring.at(shared(BridgeAbi.SHARED_NET_OUT_RING));
    }

    private static MethodHandle downcall(NativeTable table, BridgeAbi.Entry entry) {
        return table.downcall(entry.ordinal(), entry.descriptor, entry.flavour == Abi.Flavour.CRITICAL);
    }

    public static synchronized Bridge get() {
        if (instance == null) {
            String property = System.getProperty(BridgeAbi.TABLE_PROPERTY);
            if (property == null) {
                throw new IllegalStateException("mcr_bridge only runs inside mcr (" + BridgeAbi.TABLE_PROPERTY + " is not set)");
            }
            long address = Long.parseUnsignedLong(property, 16);
            NativeTable table = NativeTable.map(address, Abi.EXTENSION_MAGIC, BridgeAbi.VERSION, BridgeAbi.Entry.values().length);
            instance = new Bridge(table);
        }
        return instance;
    }

    private long shared(int id) {
        try {
            long address = (long) sharedAddress.invokeExact(id);
            if (address == 0) {
                throw new IllegalStateException("mcr has no shared object " + id);
            }
            return address;
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    public static void report(int kind, String text) {
        byte[] bytes = text.getBytes(StandardCharsets.UTF_8);
        int length = Math.min(bytes.length, MAX_REPORT_BYTES);
        Bridge bridge = get();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment segment = arena.allocate(Math.max(length, 1));
            MemorySegment.copy(bytes, 0, segment, java.lang.foreign.ValueLayout.JAVA_BYTE, 0, length);
            bridge.report.invokeExact(kind, segment.address(), length);
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    public static void info(String key, Object value) {
        report(BridgeAbi.REPORT_INFO, key + "=" + value);
    }
}
