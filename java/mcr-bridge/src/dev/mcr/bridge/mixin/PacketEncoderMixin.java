package dev.mcr.bridge.mixin;

import dev.mcr.bridge.net.BridgeChannel;
import dev.mcr.bridge.net.McrNet;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import net.minecraft.network.PacketEncoder;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.protocol.Packet;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Tags each serverbound frame with the protocol that encoded it, so a later protocol swap cannot mislabel it. */
@Mixin(PacketEncoder.class)
abstract class PacketEncoderMixin {
    @Shadow
    @Final
    private ProtocolInfo<?> protocolInfo;

    @Inject(
            method = "encode(Lio/netty/channel/ChannelHandlerContext;Lnet/minecraft/network/protocol/Packet;Lio/netty/buffer/ByteBuf;)V",
            at = @At("HEAD"))
    private void mcr$encodingPhase(ChannelHandlerContext ctx, Packet<?> packet, ByteBuf output, CallbackInfo ci) {
        if (ctx.channel() instanceof BridgeChannel bridge) {
            bridge.setEncodingPhase(McrNet.phaseOf(protocolInfo.id()));
        }
    }
}
