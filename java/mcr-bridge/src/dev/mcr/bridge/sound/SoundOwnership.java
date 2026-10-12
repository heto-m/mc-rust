package dev.mcr.bridge.sound;

import dev.mcr.bridge.net.McrNet;
import net.minecraft.client.Minecraft;

/** mcr plays the local player's movement sounds; Java's shadow player must stay silent for them. */
public final class SoundOwnership {
    public static boolean rustOwns(Object entity) {
        return McrNet.bridged() && entity == Minecraft.getInstance().player;
    }

    private SoundOwnership() {}
}
