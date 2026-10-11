package dev.mcr.boot;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

/** A Rust-owned table of function pointers: the host table or an embedder's extension table. */
public final class NativeTable {
    private final MemorySegment segment;

    private NativeTable(MemorySegment segment) {
        this.segment = segment;
    }

    public static NativeTable map(long address, int magic, int version, int entries) {
        if (address == 0) {
            throw new IllegalStateException("native table address is null");
        }
        MemorySegment header = MemorySegment.ofAddress(address).reinterpret(Abi.TABLE_HEADER_SIZE);
        int foundMagic = header.get(ValueLayout.JAVA_INT, 0);
        int foundVersion = header.get(ValueLayout.JAVA_INT, 4);
        int size = header.get(ValueLayout.JAVA_INT, 8);
        int count = header.get(ValueLayout.JAVA_INT, 12);
        check(foundMagic, foundVersion, size, count, magic, version, entries);
        return new NativeTable(MemorySegment.ofAddress(address).reinterpret(size));
    }

    static void check(int magic, int version, int size, int count, int expectedMagic, int expectedVersion, int entries) {
        if (magic != expectedMagic) {
            throw new IllegalStateException(String.format("native table magic 0x%08x, expected 0x%08x", magic, expectedMagic));
        }
        if (version != expectedVersion) {
            throw new IllegalStateException(
                    "native table version " + version + " does not match boot code version " + expectedVersion);
        }
        if (count < entries || size < Abi.TABLE_HEADER_SIZE + 8L * count) {
            throw new IllegalStateException(
                    "native table has " + count + " entries in " + size + " bytes, boot code needs " + entries);
        }
    }

    public MethodHandle downcall(int index, String descriptor, boolean critical) {
        long fn = segment.get(ValueLayout.JAVA_LONG, Abi.TABLE_HEADER_SIZE + 8L * index);
        FunctionDescriptor fd = Descriptors.parse(descriptor);
        Linker.Option[] options = critical ? new Linker.Option[] {Linker.Option.critical(false)} : new Linker.Option[0];
        return Linker.nativeLinker().downcallHandle(MemorySegment.ofAddress(fn), fd, options);
    }
}
