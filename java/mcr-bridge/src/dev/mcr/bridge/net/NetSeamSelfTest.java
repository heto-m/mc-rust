package dev.mcr.bridge.net;

import dev.mcr.bridge.Bridge;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.net.InetSocketAddress;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.multiplayer.ClientHandshakePacketListenerImpl;
import net.minecraft.client.multiplayer.LevelLoadTracker;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.network.Connection;
import net.minecraft.network.VarInt;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.network.protocol.common.ServerboundKeepAlivePacket;
import net.minecraft.network.protocol.login.LoginProtocols;

/**
 * The {@code mod-net-bridge} seam spike, kept as an opt-in check ({@code -Dmcr.bridge.test=net-seam}): a vanilla
 * login pipeline on a {@link BridgeChannel}, fed a captured {@code LoginFinished}, reported as {@code net_seam}.
 */
public final class NetSeamSelfTest {
    public static final String MODE = "net-seam";
    public static final String FRAME_PROPERTY = "mcr.bridge.test.frame";

    static boolean recording;
    private static final List<String> outbound = new ArrayList<>();
    private static boolean done;

    private NetSeamSelfTest() {}

    static void recordOutbound(int phase, ByteBuf frame) {
        outbound.add(phase + ":" + VarInt.read(frame.duplicate()));
    }

    static void maybeRun(Minecraft minecraft) {
        if (done || !(minecraft.gui.screen() instanceof TitleScreen)) {
            return;
        }
        done = true;
        StringBuilder report = new StringBuilder();
        try {
            run(minecraft, report);
        } catch (Throwable t) {
            report.append(" error=").append(t);
        }
        recording = false;
        Bridge.info("net_seam", report.toString().trim());
    }

    private static void run(Minecraft minecraft, StringBuilder report) throws Exception {
        String spec = System.getProperty(FRAME_PROPERTY, "");
        int colon = spec.indexOf(':');
        int id = Integer.parseInt(spec.substring(0, colon));
        byte[] body = Files.readAllBytes(Path.of(spec.substring(colon + 1)));

        Connection connection = new Connection(PacketFlow.CLIENTBOUND);
        BridgeChannel channel = McrNet.connectHere(InetSocketAddress.createUnresolved("localhost", 25565), connection);
        recording = true;
        ServerData server = new ServerData("seam", "localhost", ServerData.Type.OTHER);
        var listener = new ClientHandshakePacketListenerImpl(
                connection, minecraft, server, new TitleScreen(), false, null, status -> {}, new LevelLoadTracker(), null);
        long start = System.nanoTime();
        connection.initiateServerboundPlayConnection(
                "localhost", 25565, LoginProtocols.SERVERBOUND, LoginProtocols.CLIENTBOUND, listener, false);
        report.append(" setup_us=").append((System.nanoTime() - start) / 1000);
        report.append(" listener=").append(name(connection.getPacketListener()));
        report.append(" inbound_phase=").append(channel.inboundPhase);

        ByteBuf frame = Unpooled.buffer();
        VarInt.write(frame, id);
        frame.writeBytes(body);
        start = System.nanoTime();
        channel.writeInbound(frame);
        report.append(" feed_us=").append((System.nanoTime() - start) / 1000);
        report.append(" listener_after=").append(name(connection.getPacketListener()));
        report.append(" inbound_phase_after=").append(channel.inboundPhase);
        report.append(" open=").append(channel.isOpen());

        int before = outbound.size();
        Thread sender = new Thread(() -> connection.send(new ServerboundKeepAlivePacket(42)), "mcr-seam-sender");
        sender.start();
        sender.join(5000);
        int whileQueued = outbound.size();
        McrNet.loop().runTasks();
        report.append(" offthread_queued=").append(whileQueued == before);
        report.append(" offthread_sent=").append(outbound.size() == before + 1);
        report.append(" outbound=").append(String.join(",", outbound));
        channel.closedByRust = true;
        connection.disconnect(Component.literal("seam test done"));
        report.append(" closed=").append(!channel.isOpen());
    }

    private static String name(Object o) {
        return o == null ? "null" : o.getClass().getSimpleName();
    }
}
