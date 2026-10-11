package dev.mcr.bridge.net;

import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInboundHandlerAdapter;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.ChannelPromise;
import io.netty.channel.EventLoop;
import io.netty.channel.embedded.EmbeddedChannel;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import net.minecraft.network.Connection;
import net.minecraft.network.protocol.PacketFlow;

/**
 * A vanilla client pipeline without a socket: Rust's decoded frames go in one per {@code writeInbound}, and encoded
 * serverbound frames leave through {@link FrameSink} into the serverbound ring.
 */
public final class BridgeChannel extends EmbeddedChannel {
    public final Connection connection;
    private final InetSocketAddress remote;

    /** Rust session this channel mirrors; 0 while a Java-initiated join waits for Rust. */
    int session;
    /** Phase of the decoder vanilla installed last ({@code setupInboundProtocol} TAIL). */
    int inboundPhase;
    /** Phase of the encoder that produced the frame now leaving ({@code PacketEncoder.encode} HEAD). */
    int encodingPhase;
    boolean closedByRust;
    String closeReason = "Disconnected";

    private BridgeChannel(Connection connection, InetSocketAddress remote) {
        super(false, false);
        this.connection = connection;
        this.remote = remote;
    }

    /** Built off the render thread (ConnectScreen's connector); registered later on it. */
    static BridgeChannel build(Connection connection, InetSocketAddress remote) {
        BridgeChannel channel = new BridgeChannel(connection, remote);
        var pipeline = channel.pipeline();
        Connection.configureSerialization(pipeline, PacketFlow.CLIENTBOUND, false, null);
        pipeline.replace("splitter", "splitter", new FramePassThrough());
        pipeline.replace("prepender", "prepender", new FrameSink());
        connection.configurePacketHandler(pipeline);
        return channel;
    }

    public int session() {
        return session;
    }

    public int inboundPhase() {
        return inboundPhase;
    }

    public void setInboundPhase(int phase) {
        inboundPhase = phase;
    }

    public void setEncodingPhase(int phase) {
        encodingPhase = phase;
    }

    public void setCloseReason(String reason) {
        closeReason = reason;
    }

    @Override
    public EventLoop eventLoop() {
        return McrNet.loop();
    }

    @Override
    protected SocketAddress remoteAddress0() {
        return remote != null ? remote : super.remoteAddress0();
    }

    @Override
    protected void doClose() throws Exception {
        super.doClose();
        McrNet.channelClosed(this);
    }

    /** Frames arrive whole, so the vanilla splitter's place holds a pass-through under the same name. */
    static final class FramePassThrough extends ChannelInboundHandlerAdapter {}

    /** In the prepender's place: the encoded frame leaves for Rust, tagged with the phase it was encoded in. */
    static final class FrameSink extends ChannelOutboundHandlerAdapter {
        @Override
        public void write(ChannelHandlerContext ctx, Object msg, ChannelPromise promise) {
            if (msg instanceof ByteBuf frame && ctx.channel() instanceof BridgeChannel channel) {
                try {
                    McrNet.sendFrame(channel, frame);
                } finally {
                    frame.release();
                }
                promise.trySuccess();
            } else {
                ctx.write(msg, promise);
            }
        }
    }
}
