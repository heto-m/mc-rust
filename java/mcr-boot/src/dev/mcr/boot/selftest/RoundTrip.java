package dev.mcr.boot.selftest;

import dev.mcr.boot.Abi;
import dev.mcr.boot.Doorbell;
import dev.mcr.boot.Native;
import dev.mcr.boot.Ring;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

public final class RoundTrip {
    public static final int OP_ADDRESS = 1;
    public static final int OP_UPCALLS = 2;
    public static final int OP_BENCH_UPCALL = 3;
    public static final int OP_BENCH_JNI = 4;
    public static final int OP_CRITERION = 5;
    public static final int KIND_END = 0xFFF0;
    public static final int KIND_RESULT = 0xFFF1;
    public static final int EXIT_CODE = 3;

    private static final long WAIT_NANOS = 100_000_000L;
    private static final int MAX_IDLE_WAITS = 300;
    private static final int DOWNCALL_WARMUP = 2_000_000;
    private static final int DOWNCALL_CALLS = 10_000_000;
    private static final int RUST_LOOP_CALLS = 1_000_000;

    private static long jniCalls;
    private static long sink;

    public static void jniTarget() {
        jniCalls++;
    }

    public static void run() {
        Ring in = Ring.at(Native.hostCall(OP_ADDRESS, 0));
        Doorbell inBell = new Doorbell(Native.hostCall(OP_ADDRESS, 1));
        Ring out = Ring.at(Native.hostCall(OP_ADDRESS, 2));
        Doorbell outBell = new Doorbell(Native.hostCall(OP_ADDRESS, 3));

        long[] echoed = echo(in, inBell, out, outBell);
        String mismatch = versionMismatch();
        long upcalls = Native.hostCall(OP_UPCALLS, 0);
        double critical = benchCritical();
        double normal = benchNormal();
        double upcall = rustLoop(OP_BENCH_UPCALL);
        double jni = rustLoop(OP_BENCH_JNI);

        String line = String.format(Locale.ROOT,
                "MCR-SELFTEST sum=%d messages=%d bytes=%d upcalls=%d mismatch=%s jni_calls=%d"
                        + " critical_ns=%.2f normal_ns=%.2f upcall_ns=%.2f jni_ns=%.2f",
                echoed[0], echoed[1], echoed[2], upcalls, mismatch, jniCalls, critical, normal, upcall, jni);
        System.out.println(line);
        push(out, outBell, KIND_RESULT, MemorySegment.ofArray(line.getBytes(StandardCharsets.UTF_8)));
    }

    public static void exit() {
        System.exit(EXIT_CODE);
    }

    public static void bench() {
        long status = Native.hostCall(OP_CRITERION, 0);
        double critical = benchCritical();
        double normal = benchNormal();
        System.out.println(String.format(Locale.ROOT,
                "MCR-BENCH criterion_status=%d critical_ns=%.2f normal_ns=%.2f", status, critical, normal));
    }

    private static long[] echo(Ring in, Doorbell inBell, Ring out, Doorbell outBell) {
        long sum = 0;
        long messages = 0;
        long bytes = 0;
        int idle = 0;
        while (true) {
            int kind = in.poll();
            if (kind < 0) {
                int seen = inBell.seq();
                kind = in.poll();
                if (kind < 0) {
                    idle = inBell.await(seen, WAIT_NANOS) ? 0 : idle + 1;
                    if (idle > MAX_IDLE_WAITS) {
                        throw new IllegalStateException("input ring stalled");
                    }
                    continue;
                }
            }
            idle = 0;
            MemorySegment payload = in.payload();
            if (kind != KIND_END) {
                long size = payload.byteSize();
                for (long i = 0; i < size; i++) {
                    sum += Byte.toUnsignedInt(payload.get(ValueLayout.JAVA_BYTE, i));
                }
                messages++;
                bytes += size;
            }
            push(out, outBell, kind, payload);
            in.release();
            inBell.ring();
            if (kind == KIND_END) {
                return new long[] {sum, messages, bytes};
            }
        }
    }

    private static void push(Ring out, Doorbell bell, int kind, MemorySegment payload) {
        int idle = 0;
        while (!out.tryPush(kind, payload)) {
            int seen = bell.seq();
            if (out.tryPush(kind, payload)) {
                break;
            }
            idle = bell.await(seen, WAIT_NANOS) ? 0 : idle + 1;
            if (idle > MAX_IDLE_WAITS) {
                throw new IllegalStateException("output ring stalled");
            }
        }
        bell.ring();
    }

    private static String versionMismatch() {
        try {
            Native.checkHeader(Abi.MAGIC, 1, 1024, Abi.Entry.values().length, 2);
            return "accepted";
        } catch (IllegalStateException e) {
            String msg = e.getMessage();
            return msg.contains("version 1") && msg.contains("version 2") ? "ok" : "bad-message";
        }
    }

    private static double benchCritical() {
        long acc = 0;
        for (int i = 0; i < DOWNCALL_WARMUP; i++) {
            acc += Native.nopCritical(i);
        }
        long start = System.nanoTime();
        for (int i = 0; i < DOWNCALL_CALLS; i++) {
            acc += Native.nopCritical(i);
        }
        long elapsed = System.nanoTime() - start;
        sink += acc;
        return elapsed / (double) DOWNCALL_CALLS;
    }

    private static double benchNormal() {
        long acc = 0;
        for (int i = 0; i < DOWNCALL_WARMUP; i++) {
            acc += Native.nopNormal(i);
        }
        long start = System.nanoTime();
        for (int i = 0; i < DOWNCALL_CALLS; i++) {
            acc += Native.nopNormal(i);
        }
        long elapsed = System.nanoTime() - start;
        sink += acc;
        return elapsed / (double) DOWNCALL_CALLS;
    }

    private static double rustLoop(int op) {
        Native.hostCall(op, RUST_LOOP_CALLS / 4);
        long nanos = Native.hostCall(op, RUST_LOOP_CALLS);
        return nanos < 0 ? -1 : nanos / (double) RUST_LOOP_CALLS;
    }

    public static long sink() {
        return sink;
    }

    private RoundTrip() {}
}
