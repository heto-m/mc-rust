package dev.mcr.bridge;

import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;

/** Rust owns the socket; Java only asks for a connection. The packet bridge arrives in mod-net-bridge. */
public final class McrNet {
    public static void requestConnect(ServerAddress address, ServerData server) {
        Bridge.report(BridgeAbi.REPORT_CONNECT, address.getHost() + ":" + address.getPort() + "\t" + server.name);
    }

    private McrNet() {}
}
