package dev.mcr.testmod;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

public final class McrTestMod implements ClientModInitializer {
    public static final String MESSAGE = "hello from mcr_testmod";
    public static final String COMMAND = "time query day";
    /** Fabric's client command dispatcher only exists once the server's command tree arrived. */
    private static final int COMMAND_DELAY_TICKS = 40;

    private static int commandIn;

    public record Hello(String message) implements CustomPacketPayload {
        public static final Type<Hello> TYPE = new Type<>(Identifier.fromNamespaceAndPath("mcr_testmod", "hello"));
        public static final StreamCodec<RegistryFriendlyByteBuf, Hello> CODEC =
                StreamCodec.composite(ByteBufCodecs.STRING_UTF8, Hello::message, Hello::new);

        @Override
        public Type<Hello> type() {
            return TYPE;
        }
    }

    @Override
    public void onInitializeClient() {
        PayloadTypeRegistry.serverboundPlay().register(Hello.TYPE, Hello.CODEC);
        ClientPlayConnectionEvents.JOIN.register((handler, sender, client) -> {
            sender.sendPacket(new Hello(MESSAGE));
            commandIn = COMMAND_DELAY_TICKS;
        });
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            if (commandIn > 0 && --commandIn == 0 && client.getConnection() != null) {
                client.getConnection().sendCommand(COMMAND);
            }
        });
    }
}
