package dev.mcr.bridge.sound;

import dev.mcr.boot.Ring;
import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.BridgeAbi;
import java.io.IOException;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.VarHandle;
import java.lang.ref.Cleaner;
import java.nio.ByteBuffer;
import javax.sound.sampled.AudioFormat;
import net.minecraft.client.Options;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.AudioStream;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.phys.Vec3;
import org.jspecify.annotations.Nullable;

/** OpenAL's replacement: channel ids are negative slots, operations go to mcr's sound ring, PCM by downcall. */
public final class BridgeAudio {
    private static final VarHandle INT = ValueLayout.JAVA_INT.varHandle();
    private static final Cleaner CLEANER = Cleaner.create();
    private static final int MAX_PUSH = 64 << 20;
    private static final int PUSH_ROUNDS = 64;
    private static final int RING_SPINS = 2000;
    private static final SoundSource[] SOURCES = SoundSource.values();
    private static final boolean TRACE = Boolean.getBoolean("mcr.sound.trace");

    private static @Nullable BridgeAudio instance;

    private final Ring ring;
    private final MemorySegment states;
    private final MemorySegment control;
    private final MethodHandle upload;
    private final MethodHandle free;
    private final MethodHandle streamOpen;
    private final MethodHandle streamPush;
    private final MemorySegment command = Arena.global().allocate(SoundAbi.SND_SIZE, 8);
    private MemorySegment scratch = MemorySegment.NULL;

    private final boolean[] used = new boolean[SoundAbi.SOUND_SLOTS];
    private final int[] generation = new int[SoundAbi.SOUND_SLOTS];
    private final int[] serial = new int[SoundAbi.SOUND_SLOTS];
    private final @Nullable AudioStream[] streams = new AudioStream[SoundAbi.SOUND_SLOTS];
    private final @Nullable ByteBuffer[] leftover = new ByteBuffer[SoundAbi.SOUND_SLOTS];
    private final int[] frameBytes = new int[SoundAbi.SOUND_SLOTS];
    private int cursor;
    private final float[] sharedVolumes = new float[SoundAbi.SOURCES];
    private int volumeSeq;

    private BridgeAudio(Bridge bridge) {
        this.ring = Ring.at(bridge.shared(BridgeAbi.SHARED_SOUND_RING));
        this.states = MemorySegment.ofAddress(bridge.shared(BridgeAbi.SHARED_SOUND_STATES))
                .reinterpret(4L * SoundAbi.SOUND_SLOTS);
        this.control = MemorySegment.ofAddress(bridge.shared(BridgeAbi.SHARED_SOUND_CONTROL))
                .reinterpret(SoundAbi.CTL_SIZE);
        this.upload = bridge.downcall(BridgeAbi.Entry.SOUND_UPLOAD);
        this.free = bridge.downcall(BridgeAbi.Entry.SOUND_FREE);
        this.streamOpen = bridge.downcall(BridgeAbi.Entry.SOUND_STREAM_OPEN);
        this.streamPush = bridge.downcall(BridgeAbi.Entry.SOUND_STREAM_PUSH);
        java.util.Arrays.fill(sharedVolumes, Float.NaN);
    }

    private static synchronized BridgeAudio get() {
        if (instance == null) {
            instance = new BridgeAudio(Bridge.get());
        }
        return instance;
    }

    private static int slotOf(int channel) {
        int slot = -channel - 1;
        return slot >= 0 && slot < SoundAbi.SOUND_SLOTS ? slot : -1;
    }

    private static int word(int generation, int state) {
        return ((generation & SoundAbi.GENERATION_MASK) << 8) | state;
    }

    private int state(int slot) {
        return (int) INT.getVolatile(states, 4L * slot) & 0xFF;
    }

    private void setState(int slot, int state) {
        INT.setVolatile(states, 4L * slot, word(generation[slot], state));
    }

    private boolean moveState(int slot, int from, int to) {
        return INT.compareAndSet(states, 4L * slot, word(generation[slot], from), word(generation[slot], to));
    }

    private boolean attached() {
        return (int) INT.getVolatile(control, (long) SoundAbi.CTL_ATTACHED) != 0;
    }

    private void send(int kind, int slot, int arg, float x, float y, float z) {
        command.set(ValueLayout.JAVA_INT, SoundAbi.SND_SLOT, slot);
        command.set(ValueLayout.JAVA_INT, SoundAbi.SND_ARG, arg);
        command.set(ValueLayout.JAVA_FLOAT, SoundAbi.SND_X, x);
        command.set(ValueLayout.JAVA_FLOAT, SoundAbi.SND_Y, y);
        command.set(ValueLayout.JAVA_FLOAT, SoundAbi.SND_Z, z);
        for (int i = 0; i < RING_SPINS; i++) {
            if (ring.tryPush(kind, command)) {
                return;
            }
            Thread.onSpinWait();
        }
        INT.getAndAdd(control, (long) SoundAbi.CTL_DROPPED, 1);
    }

    private void send(int kind, int slot, int arg) {
        send(kind, slot, arg, 0, 0, 0);
    }

    /** {@code Channel.create}: a new channel id, or 0 when no slot is free or no mixer listens. */
    // SoundEngine.stopAll releases channels from the render thread, so every entry point locks.
    public static int create() {
        BridgeAudio a = get();
        synchronized (a) {
            if (!a.attached()) {
                return 0;
            }
            for (int i = 0; i < SoundAbi.SOUND_SLOTS; i++) {
                int slot = (a.cursor + i) % SoundAbi.SOUND_SLOTS;
                if (!a.used[slot]) {
                    a.cursor = slot + 1;
                    a.used[slot] = true;
                    a.generation[slot] = (a.generation[slot] + 1) & SoundAbi.GENERATION_MASK;
                    a.setState(slot, SoundAbi.STATE_INITIAL);
                    a.send(SoundAbi.SND_CREATE, slot, 0);
                    return -slot - 1;
                }
            }
            return 0;
        }
    }

    public static void destroy(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot < 0 || !a.used[slot]) {
                return;
            }
            a.closeStream(slot);
            a.setState(slot, SoundAbi.STATE_STOPPED);
            a.send(SoundAbi.SND_DESTROY, slot, 0);
            a.used[slot] = false;
        }
    }

    public static void play(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot < 0) {
                return;
            }
            a.generation[slot] = (a.generation[slot] + 1) & SoundAbi.GENERATION_MASK;
            a.setState(slot, SoundAbi.STATE_PLAYING);
            a.send(SoundAbi.SND_PLAY, slot, a.generation[slot]);
        }
    }

    public static void pause(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot >= 0 && a.moveState(slot, SoundAbi.STATE_PLAYING, SoundAbi.STATE_PAUSED)) {
                a.send(SoundAbi.SND_PAUSE, slot, 0);
            }
        }
    }

    public static void unpause(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot >= 0 && a.moveState(slot, SoundAbi.STATE_PAUSED, SoundAbi.STATE_PLAYING)) {
                a.send(SoundAbi.SND_RESUME, slot, 0);
            }
        }
    }

    public static void stop(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot >= 0) {
                a.setState(slot, SoundAbi.STATE_STOPPED);
                a.send(SoundAbi.SND_STOP, slot, 0);
            }
        }
    }

    /** Lock-free: the mixer publishes ends straight into the state table. */
    public static boolean hasState(int channel, int state) {
        int slot = slotOf(channel);
        return slot >= 0 && get().state(slot) == state;
    }

    public static void position(int channel, Vec3 p) {
        value(channel, SoundAbi.SND_POSITION, 0, (float) p.x, (float) p.y, (float) p.z);
    }

    public static void value(int channel, int kind, int arg, float x, float y, float z) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot >= 0) {
                a.send(kind, slot, arg, x, y, z);
            }
        }
    }

    public static void attachBuffer(int channel, int buffer) {
        value(channel, SoundAbi.SND_BUFFER, buffer, 0, 0, 0);
    }

    private static int format(AudioFormat f) {
        int bits = f.getSampleSizeInBits();
        AudioFormat.Encoding e = f.getEncoding();
        if ((bits != 8 && bits != 16)
                || (e != AudioFormat.Encoding.PCM_SIGNED && e != AudioFormat.Encoding.PCM_UNSIGNED)) {
            return -1;
        }
        return bits
                | (e == AudioFormat.Encoding.PCM_SIGNED ? SoundAbi.FORMAT_SIGNED : 0)
                | (f.isBigEndian() ? SoundAbi.FORMAT_BIG_ENDIAN : 0);
    }

    /** Native view of {@code data}'s remaining bytes; heap buffers are copied into a reused scratch. */
    private MemorySegment nativeView(ByteBuffer data, int length) {
        if (data.isDirect()) {
            return MemorySegment.ofBuffer(data).asSlice(0, length);
        }
        if (scratch.byteSize() < length) {
            scratch = Arena.ofAuto().allocate(Math.max(length, 64 << 10), 8);
        }
        MemorySegment.copy(MemorySegment.ofBuffer(data), 0, scratch, 0, length);
        return scratch.asSlice(0, length);
    }

    /** Copies a static buffer's PCM into Rust; returns its id or 0. */
    public static int upload(ByteBuffer data, AudioFormat format) {
        int flags = format(format);
        int length = data.remaining();
        if (flags < 0 || length <= 0 || length > MAX_PUSH) {
            return 0;
        }
        BridgeAudio a = get();
        synchronized (a) {
            try {
                MemorySegment view = a.nativeView(data, length);
                return (int) a.upload.invokeExact(
                        view.address(), length, (int) format.getSampleRate(), format.getChannels(), flags);
            } catch (Throwable t) {
                return 0;
            }
        }
    }

    /** Frees the Rust copy once, explicitly or when the {@code SoundBuffer} is collected. */
    public static Cleaner.Cleanable track(Object owner, int buffer) {
        return CLEANER.register(owner, () -> free(buffer));
    }

    private static void free(int buffer) {
        BridgeAudio a = get();
        try {
            a.free.invokeExact(buffer);
        } catch (Throwable ignored) {
        }
    }

    public static void attachStream(int channel, AudioStream stream) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot < 0) {
                close(stream);
                return;
            }
            a.closeStream(slot);
            AudioFormat f = stream.getFormat();
            int flags = format(f);
            int opened = 0;
            int next = a.serial[slot] + 1;
            if (flags >= 0) {
                try {
                    opened = (int) a.streamOpen.invokeExact(
                            slot, next, (int) f.getSampleRate(), f.getChannels(), flags);
                } catch (Throwable t) {
                    opened = 0;
                }
            }
            a.serial[slot] = next;
            if (opened == 0) {
                close(stream);
                return;
            }
            a.streams[slot] = stream;
            a.frameBytes[slot] = Math.max(1, f.getChannels() * f.getSampleSizeInBits() / 8);
            a.send(SoundAbi.SND_STREAM, slot, next);
            a.pump(slot);
        }
    }

    public static void updateStream(int channel) {
        BridgeAudio a = get();
        synchronized (a) {
            int slot = slotOf(channel);
            if (slot >= 0) {
                a.pump(slot);
            }
        }
    }

    private int push(int slot, MemorySegment data, int length) {
        try {
            return (int) streamPush.invokeExact(slot, serial[slot], data.address(), length);
        } catch (Throwable t) {
            return -1;
        }
    }

    /** Tops the stream's ring up to three quarters, as vanilla refills processed AL buffers. */
    private void pump(int slot) {
        AudioStream stream = streams[slot];
        if (stream == null) {
            return;
        }
        int want = push(slot, MemorySegment.NULL, 0);
        for (int round = 0; round < PUSH_ROUNDS && want > 0; round++) {
            ByteBuffer pending = leftover[slot];
            if (pending == null) {
                try {
                    pending = stream.read(want);
                } catch (IOException e) {
                    pending = null;
                }
                if (pending == null || !pending.hasRemaining()) {
                    push(slot, MemorySegment.NULL, SoundAbi.STREAM_END);
                    close(stream);
                    streams[slot] = null;
                    return;
                }
            }
            int length = Math.min(pending.remaining(), MAX_PUSH);
            int accepted = push(slot, nativeView(pending, length), length);
            if (accepted < 0) {
                closeStream(slot);
                return;
            }
            pending.position(pending.position() + accepted);
            boolean whole = pending.remaining() >= frameBytes[slot];
            leftover[slot] = whole ? pending : null;
            if (accepted < length && whole) {
                return;
            }
            want -= accepted;
        }
    }

    private void closeStream(int slot) {
        AudioStream stream = streams[slot];
        leftover[slot] = null;
        if (stream != null) {
            streams[slot] = null;
            close(stream);
            push(slot, MemorySegment.NULL, SoundAbi.STREAM_END);
        }
    }

    private static void close(AudioStream stream) {
        try {
            stream.close();
        } catch (IOException ignored) {
        }
    }

    /** With {@code -Dmcr.sound.trace=true}, reports every sound Java asks to play as {@code sound_play}. */
    public static void trace(SoundInstance instance) {
        if (TRACE) {
            Bridge.info("sound_play", instance.getIdentifier() + " " + instance.getX() + " " + instance.getY() + " " + instance.getZ());
        }
    }

    /** Java's sliders drive mcr's own sounds too; written once per client tick, sequenced on change. */
    public static void shareVolumes(Options options) {
        BridgeAudio a = get();
        synchronized (a) {
            boolean changed = false;
            for (int i = 0; i < SoundAbi.SOURCES && i < SOURCES.length; i++) {
                float v = options.getSoundSourceVolume(SOURCES[i]);
                if (Float.compare(v, a.sharedVolumes[i]) != 0) {
                    a.sharedVolumes[i] = v;
                    a.control.set(ValueLayout.JAVA_FLOAT, SoundAbi.CTL_VOLUMES + 4L * i, v);
                    changed = true;
                }
            }
            if (changed) {
                a.volumeSeq = a.volumeSeq + 1 == 0 ? 1 : a.volumeSeq + 1;
                INT.setRelease(a.control, (long) SoundAbi.CTL_VOLUME_SEQ, a.volumeSeq);
            }
        }
    }
}
