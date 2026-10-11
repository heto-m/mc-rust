package dev.mcr.bridge.mixin;

import dev.mcr.bridge.net.BridgeChannel;
import dev.mcr.bridge.net.McrNet;
import io.netty.channel.Channel;
import io.netty.channel.ChannelFuture;
import java.net.InetSocketAddress;
import net.minecraft.network.Connection;
import net.minecraft.network.DisconnectionDetails;
import net.minecraft.network.PacketListener;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.server.network.EventLoopGroupHolder;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Connection.class)
abstract class ConnectionMixin {
    @Shadow
    private Channel channel;

    @Inject(
            method = "connect(Ljava/net/InetSocketAddress;Lnet/minecraft/server/network/EventLoopGroupHolder;Lnet/minecraft/network/Connection;)Lio/netty/channel/ChannelFuture;",
            at = @At("HEAD"),
            cancellable = true)
    private static void mcr$bridgeConnect(
            InetSocketAddress address,
            EventLoopGroupHolder group,
            Connection connection,
            CallbackInfoReturnable<ChannelFuture> cir) {
        cir.setReturnValue(McrNet.connect(address, connection));
    }

    @Inject(method = "setupInboundProtocol", at = @At("TAIL"))
    private <T extends PacketListener> void mcr$inboundPhase(ProtocolInfo<T> protocol, T listener, CallbackInfo ci) {
        if (channel instanceof BridgeChannel bridge) {
            bridge.setInboundPhase(McrNet.phaseOf(protocol.id()));
        }
    }

    @Inject(method = "disconnect(Lnet/minecraft/network/DisconnectionDetails;)V", at = @At("HEAD"))
    private void mcr$closeReason(DisconnectionDetails details, CallbackInfo ci) {
        if (channel instanceof BridgeChannel bridge) {
            bridge.setCloseReason(details.reason().getString());
        }
    }
}
