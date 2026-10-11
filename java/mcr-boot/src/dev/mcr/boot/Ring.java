package dev.mcr.boot;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;

/** One end of an mcr-jvm ring: an instance is either the writer or the reader, never both. */
public final class Ring {
    private static final VarHandle LONG = ValueLayout.JAVA_LONG.varHandle();
    private static final long WRITE = Abi.RING_WRITE_OFFSET;
    private static final long READ = Abi.RING_READ_OFFSET;
    private static final long HEADER = Abi.RING_HEADER_SIZE;
    private static final long RECORD = Abi.RECORD_HEADER_SIZE;

    private final MemorySegment segment;
    private final MemorySegment data;
    private final long capacity;
    private final long mask;
    private final long maxPayload;

    private long write;
    private long read;
    private long readCache;
    private long writeCache;
    private long pendingTotal = -1;
    private int pendingKind;
    private long pendingOffset;
    private long pendingLength;

    private Ring(MemorySegment segment, long capacity) {
        this.segment = segment;
        this.data = segment.asSlice(HEADER, capacity);
        this.capacity = capacity;
        this.mask = capacity - 1;
        this.maxPayload = capacity / 2 - RECORD;
        this.write = (long) LONG.getAcquire(segment, WRITE);
        this.read = (long) LONG.getAcquire(segment, READ);
        this.readCache = read;
        this.writeCache = write;
    }

    public static Ring at(long address) {
        MemorySegment header = MemorySegment.ofAddress(address).reinterpret(8);
        int magic = header.get(ValueLayout.JAVA_INT, 0);
        long capacity = Integer.toUnsignedLong(header.get(ValueLayout.JAVA_INT, 4));
        if (magic != Abi.RING_MAGIC || Long.bitCount(capacity) != 1 || capacity < 256) {
            throw new IllegalStateException(String.format("not an mcr ring at 0x%x", address));
        }
        return new Ring(MemorySegment.ofAddress(address).reinterpret(HEADER + capacity), capacity);
    }

    public long maxPayload() {
        return maxPayload;
    }

    private static long recordSize(long length) {
        return (RECORD + length + 7) & ~7L;
    }

    public boolean tryPush(int kind, MemorySegment payload) {
        long length = payload.byteSize();
        if (kind < 0 || kind >= Abi.PADDING_KIND) {
            throw new IllegalArgumentException("bad message kind " + kind);
        }
        if (length > maxPayload) {
            throw new IllegalArgumentException("payload " + length + " exceeds ring maximum " + maxPayload);
        }
        long total = recordSize(length);
        long pos = write & mask;
        long tail = capacity - pos;
        long needed = total <= tail ? total : tail + total;
        if (needed > capacity - (write - readCache)) {
            readCache = (long) LONG.getAcquire(segment, READ);
            if (needed > capacity - (write - readCache)) {
                return false;
            }
        }
        if (total > tail) {
            writeHeader(pos, (int) (tail - RECORD), Abi.PADDING_KIND);
            write += tail;
            pos = 0;
        }
        writeHeader(pos, (int) length, kind);
        MemorySegment.copy(payload, 0, data, pos + RECORD, length);
        write += total;
        LONG.setRelease(segment, WRITE, write);
        return true;
    }

    private void writeHeader(long pos, int length, int kind) {
        data.set(ValueLayout.JAVA_INT, pos, length);
        data.set(ValueLayout.JAVA_SHORT, pos + 4, (short) kind);
        data.set(ValueLayout.JAVA_SHORT, pos + 6, (short) 0);
    }

    /** Returns the next message kind, or -1 when empty; the payload stays valid until {@link #release()}. */
    public int poll() {
        if (pendingTotal >= 0) {
            return pendingKind;
        }
        while (true) {
            if (read == writeCache) {
                writeCache = (long) LONG.getAcquire(segment, WRITE);
                if (read == writeCache) {
                    return -1;
                }
            }
            long pos = read & mask;
            long length = Integer.toUnsignedLong(data.get(ValueLayout.JAVA_INT, pos));
            int kind = Short.toUnsignedInt(data.get(ValueLayout.JAVA_SHORT, pos + 4));
            long total = recordSize(length);
            boolean fits = length <= capacity - pos - RECORD && total <= writeCache - read;
            if (!fits || (kind != Abi.PADDING_KIND && length > maxPayload)) {
                throw new IllegalStateException("corrupt ring record at " + pos + " (length " + length + ")");
            }
            if (kind == Abi.PADDING_KIND) {
                read += total;
                LONG.setRelease(segment, READ, read);
                continue;
            }
            pendingTotal = total;
            pendingKind = kind;
            pendingOffset = pos + RECORD;
            pendingLength = length;
            return kind;
        }
    }

    public MemorySegment payload() {
        if (pendingTotal < 0) {
            throw new IllegalStateException("no message polled");
        }
        return data.asSlice(pendingOffset, pendingLength);
    }

    public void release() {
        if (pendingTotal < 0) {
            throw new IllegalStateException("no message polled");
        }
        read += pendingTotal;
        pendingTotal = -1;
        LONG.setRelease(segment, READ, read);
    }
}
