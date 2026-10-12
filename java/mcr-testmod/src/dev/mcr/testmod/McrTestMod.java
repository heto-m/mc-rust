package dev.mcr.testmod;

import com.mojang.blaze3d.platform.InputConstants;
import dev.mcr.bridge.Bridge;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.minecraft.client.KeyMapping;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class McrTestMod implements ClientModInitializer {
    static final Logger LOGGER = LoggerFactory.getLogger("mcr_testmod");
    public static final String MESSAGE = "hello from mcr_testmod";
    public static final String COMMAND = "time query day";
    /** Fabric's client command dispatcher only exists once the server's command tree arrived. */
    private static final int COMMAND_DELAY_TICKS = 40;

    /** `-Dmcr.testmod.hangAfterTicks=N`: N client ticks after loading, the render thread blocks for good. */
    private static final int HANG_AFTER = Integer.getInteger("mcr.testmod.hangAfterTicks", -1);

    private static final KeyMapping PING = new KeyMapping(
            "key.mcr_testmod.ping",
            InputConstants.Type.KEYBOARD,
            InputConstants.KEY_K,
            KeyMapping.Category.register(Identifier.fromNamespaceAndPath("mcr_testmod", "test")));

    private static int commandIn;
    private static int pings;
    private static int loadedTicks;

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
        MarkerLayer.register();
        PayloadTypeRegistry.serverboundPlay().register(Hello.TYPE, Hello.CODEC);
        KeyMappingHelper.registerKeyMapping(PING);
        ClientPlayConnectionEvents.JOIN.register((handler, sender, client) -> {
            sender.sendPacket(new Hello(MESSAGE));
            commandIn = COMMAND_DELAY_TICKS;
        });
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            GuiCapture.tick(client);
            while (PING.consumeClick()) {
                Bridge.info("testmod_key", ++pings);
            }
            if (SoundSelfTest.enabled()) {
                SoundSelfTest.tick(client);
            }
            if (HANG_AFTER >= 0 && client.isGameLoadFinished() && client.gui.overlay() == null && ++loadedTicks > HANG_AFTER) {
                LOGGER.warn("mcr_testmod: blocking the render thread for good");
                while (true) {
                    java.util.concurrent.locks.LockSupport.park();
                }
            }
            if (commandIn > 0 && --commandIn == 0 && client.getConnection() != null) {
                client.getConnection().sendCommand(COMMAND);
            }
        });
    }
}
