package dev.mcr.bridge.net;

import dev.mcr.boot.Ring;
import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrBridge;
import dev.mcr.bridge.mixin.ConnectionAccessor;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelFuture;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.network.Connection;
import net.minecraft.network.ConnectionProtocol;
import net.minecraft.network.chat.Component;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Java's half of the packet bridge. Everything here runs on the render thread except {@link #connect}, which
 * ConnectScreen's connector thread calls; its channel is handed over and activated on the render thread.
 */
public final class McrNet {
    private static final Logger LOGGER = LoggerFactory.getLogger("mcr-net");
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfByte BYTE = ValueLayout.JAVA_BYTE;
    private static final long MAX_OUT_SPILL = 16 << 20;
    private static final boolean SELF_TEST = NetSeamSelfTest.MODE.equals(System.getProperty(McrBridge.TEST_PROPERTY));

    private static RenderThreadLoop loop;
    private static Bridge bridge;
    private static final ThreadLocal<Boolean> CONNECTOR = ThreadLocal.withInitial(() -> false);
    private static volatile BridgeChannel pending;
    private static volatile boolean pendingReady;

    private static BridgeChannel channel;
    private static int openSession;
    private static int connectingSession;
    private static String closeReason = "Disconnected";

    private static MemorySegment scratch = Arena.global().allocate(64 << 10, 8);
    private static final ArrayDeque<byte[]> outSpill = new ArrayDeque<>();
    private static final ArrayDeque<Integer> outSpillKinds = new ArrayDeque<>();
    private static long outSpillBytes;

    static long framesFed;
    static long feedNanos;
    static long phaseMismatches;
    static long outDropped;

    private McrNet() {}

    /** First frame on the render thread. */
    public static void init(Thread renderThread) {
        if (loop == null) {
            loop = new RenderThreadLoop(renderThread);
            bridge = Bridge.get();
        }
    }

    public static RenderThreadLoop loop() {
        RenderThreadLoop l = loop;
        if (l == null) {
            throw new IllegalStateException("mcr net bridge used before the first frame");
        }
        return l;
    }

    public static boolean bridged() {
        return channel != null;
    }

    public static int phaseOf(ConnectionProtocol protocol) {
        return switch (protocol) {
            case LOGIN -> BridgeAbi.PHASE_LOGIN;
            case CONFIGURATION -> BridgeAbi.PHASE_CONFIGURATION;
            case PLAY -> BridgeAbi.PHASE_PLAY;
            default -> 0;
        };
    }

    // ---- connection setup (connector thread) ----

    public static void connectorStarted() {
        CONNECTOR.set(true);
    }

    public static void connectorFinished() {
        CONNECTOR.set(false);
        pendingReady = true;
    }

    /** Replaces {@code Connection.connect}: no socket, a bridge channel the render thread activates. */
    public static ChannelFuture connect(InetSocketAddress address, Connection connection) {
        if (!CONNECTOR.get()) {
            throw new IllegalStateException("mcr: Java opens no sockets; only ConnectScreen joins are bridged");
        }
        BridgeChannel ch = BridgeChannel.build(connection, address);
        pendingReady = false;
        pending = ch;
        return ch.newSucceededFuture();
    }

    /** Builds and activates a channel on the render thread (the seam self-test). */
    static BridgeChannel connectHere(InetSocketAddress address, Connection connection) {
        BridgeChannel ch = BridgeChannel.build(connection, address);
        activate(ch);
        return ch;
    }

    private static void activate(BridgeChannel ch) {
        try {
            ch.register();
        } catch (Exception e) {
            throw new IllegalStateException("bridge channel registration failed", e);
        }
        ((ConnectionAccessor) ch.connection).mcr$flushQueue();
        ch.runPendingTasks();
        loop.runTasks();
    }

    private static void activatePending() {
        BridgeChannel ch = pending;
        if (ch == null || !pendingReady) {
            return;
        }
        pending = null;
        pendingReady = false;
        if (channel != null && channel != ch) {
            channel.connection.disconnect(Component.translatable("disconnect.lost"));
        }
        activate(ch);
        channel = ch;
        if (connectingSession != 0) {
            int session = connectingSession;
            connectingSession = 0;
            if (session == openSession) {
                ch.session = session;
            } else {
                ch.closedByRust = true;
                ch.connection.disconnect(Component.literal(closeReason));
                channel = null;
            }
            return;
        }
        InetSocketAddress remote = (InetSocketAddress) ch.remoteAddress();
        Bridge.report(BridgeAbi.REPORT_CONNECT, remote.getHostString() + ":" + remote.getPort() + "\t" + remote.getHostString());
    }

    // ---- per frame (render thread) ----

    /** Called from {@code awaitFrame} once the frame messages are drained. */
    public static void pump(Minecraft minecraft) {
        if (loop == null) {
            return;
        }
        loop.runTasks();
        activatePending();
        drainInbound(minecraft);
        flushOutSpill();
        if (SELF_TEST) {
            NetSeamSelfTest.maybeRun(minecraft);
        }
    }

    public static void endOfTick() {
        if (loop == null) {
            return;
        }
        loop.runTasks();
        flushOutSpill();
    }

    private static void drainInbound(Minecraft minecraft) {
        Ring in = bridge.netIn;
        int kind;
        while ((kind = in.poll()) >= 0) {
            MemorySegment p = in.payload();
            if (p.byteSize() >= BridgeAbi.NET_HEADER) {
                int session = p.get(INT, BridgeAbi.NET_SESSION);
                boolean wait = switch (kind) {
                    case BridgeAbi.NET_FRAME -> feed(session, p);
                    case BridgeAbi.NET_OPEN -> {
                        open(minecraft, session, p);
                        yield false;
                    }
                    case BridgeAbi.NET_CLOSE -> {
                        close(session, utf8(p, BridgeAbi.NET_HEADER));
                        yield false;
                    }
                    case BridgeAbi.NET_PLAYER -> {
                        if (channel != null && channel.session == session) {
                            PlayerShadow.receive(p);
                        }
                        yield false;
                    }
                    case BridgeAbi.NET_PROBE -> {
                        NetProbe.answer(minecraft, p, channel == null ? 0 : channel.session);
                        yield false;
                    }
                    default -> false;
                };
                if (wait) {
                    return;
                }
            }
            in.release();
        }
    }

    /** True when the frame must stay in the ring until this session's channel is active. */
    private static boolean feed(int session, MemorySegment p) {
        BridgeChannel ch = channel;
        if (ch == null || ch.session != session) {
            return session == openSession && (connectingSession == session || pending != null || ch != null && ch.session == 0);
        }
        if (!ch.isOpen()) {
            return false;
        }
        int tag = p.get(BYTE, BridgeAbi.NET_TAG);
        if (tag != ch.inboundPhase) {
            if (phaseMismatches++ < 16) {
                LOGGER.warn("dropping a frame tagged phase {} while Java decodes phase {}", tag, ch.inboundPhase);
            }
            return false;
        }
        long start = System.nanoTime();
        MemorySegment body = p.asSlice(BridgeAbi.NET_HEADER);
        ByteBuf frame = ch.alloc().buffer((int) body.byteSize());
        frame.writeBytes(body.asByteBuffer());
        try {
            ch.writeInbound(frame);
        } catch (Throwable t) {
            LOGGER.error("bridge channel failed on an inbound frame", t);
            ch.connection.disconnect(Component.literal("mcr bridge: " + t));
        }
        feedNanos += System.nanoTime() - start;
        framesFed++;
        return false;
    }

    private static void open(Minecraft minecraft, int session, MemorySegment p) {
        int port = p.get(INT, BridgeAbi.OPEN_PORT);
        String host = utf8(p, BridgeAbi.OPEN_HOST);
        openSession = session;
        BridgeChannel ch = channel;
        if (ch != null && ch.session == 0 && ch.isOpen()) {
            ch.session = session;
            return;
        }
        if (ch != null) {
            ch.closedByRust = true;
            ch.connection.disconnect(Component.translatable("disconnect.lost"));
            channel = null;
        }
        connectingSession = session;
        if (pending != null) {
            return;
        }
        ServerData data = new ServerData(host, host + ":" + port, ServerData.Type.OTHER);
        Screen parent = minecraft.gui.screen();
        if (parent == null || parent instanceof ConnectScreen) {
            parent = new TitleScreen();
        }
        ConnectScreen.startConnecting(parent, minecraft, new ServerAddress(host, port), data, false, null);
    }

    private static void close(int session, String reason) {
        closeReason = reason;
        if (openSession == session) {
            openSession = 0;
        }
        BridgeChannel ch = channel;
        if (ch != null && ch.session == session) {
            ch.closedByRust = true;
            channel = null;
            PlayerShadow.reset();
            ch.connection.disconnect(Component.literal(reason));
        }
    }

    /** {@code doClose} of a bridge channel, on the render thread. */
    static void channelClosed(BridgeChannel ch) {
        if (channel == ch) {
            channel = null;
            PlayerShadow.reset();
        }
        if (!ch.closedByRust && ch.session != 0) {
            push(BridgeAbi.OUT_DISCONNECT, 0, ch.session, ch.closeReason.getBytes(StandardCharsets.UTF_8));
        }
    }

    // ---- serverbound ----

    static void sendFrame(BridgeChannel ch, ByteBuf frame) {
        int len = frame.readableBytes();
        MemorySegment seg = reserve(BridgeAbi.NET_HEADER + len);
        header(seg, ch.encodingPhase, ch.session);
        frame.getBytes(frame.readerIndex(), seg.asSlice(BridgeAbi.NET_HEADER, len).asByteBuffer());
        pushSegment(BridgeAbi.OUT_FRAME, seg.asSlice(0, BridgeAbi.NET_HEADER + len));
        if (NetSeamSelfTest.recording) {
            NetSeamSelfTest.recordOutbound(ch.encodingPhase, frame);
        }
    }

    public static void sendChat(int kind, String text) {
        BridgeChannel ch = channel;
        push(BridgeAbi.OUT_CHAT, kind, ch == null ? 0 : ch.session, text.getBytes(StandardCharsets.UTF_8));
    }

    static void push(int kind, int tag, int session, byte[] body) {
        MemorySegment seg = reserve(BridgeAbi.NET_HEADER + body.length);
        header(seg, tag, session);
        MemorySegment.copy(body, 0, seg, BYTE, BridgeAbi.NET_HEADER, body.length);
        pushSegment(kind, seg.asSlice(0, BridgeAbi.NET_HEADER + body.length));
    }

    static MemorySegment reserve(long size) {
        if (scratch.byteSize() < size) {
            scratch = Arena.ofAuto().allocate(Math.max(size, scratch.byteSize() * 2), 8);
        }
        return scratch;
    }

    static void header(MemorySegment seg, int tag, int session) {
        seg.set(INT, 0, 0);
        seg.set(BYTE, BridgeAbi.NET_TAG, (byte) tag);
        seg.set(INT, BridgeAbi.NET_SESSION, session);
    }

    static void pushSegment(int kind, MemorySegment message) {
        Ring out = bridge.netOut;
        if (message.byteSize() > out.maxPayload()) {
            outDropped++;
            LOGGER.warn("dropping a {} byte serverbound message (ring maximum {})", message.byteSize(), out.maxPayload());
            return;
        }
        if (outSpill.isEmpty() && out.tryPush(kind, message)) {
            return;
        }
        if (outSpillBytes + message.byteSize() > MAX_OUT_SPILL) {
            outDropped++;
            return;
        }
        outSpill.add(message.toArray(BYTE));
        outSpillKinds.add(kind);
        outSpillBytes += message.byteSize();
    }

    private static void flushOutSpill() {
        Ring out = bridge.netOut;
        while (!outSpill.isEmpty()) {
            byte[] bytes = outSpill.peek();
            if (!out.tryPush(outSpillKinds.peek(), MemorySegment.ofArray(bytes))) {
                return;
            }
            outSpill.poll();
            outSpillKinds.poll();
            outSpillBytes -= bytes.length;
        }
    }

    static String utf8(MemorySegment p, long offset) {
        if (p.byteSize() <= offset) {
            return "";
        }
        return new String(p.asSlice(offset).toArray(BYTE), StandardCharsets.UTF_8);
    }
}
