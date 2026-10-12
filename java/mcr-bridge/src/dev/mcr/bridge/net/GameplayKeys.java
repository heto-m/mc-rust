package dev.mcr.bridge.net;

import net.minecraft.client.KeyMapping;
import net.minecraft.client.Options;

/** Vanilla gameplay keys belong to Rust; screen keys and mod-registered mappings stay Java's. */
public final class GameplayKeys {
    private GameplayKeys() {}

    public static void suppress(Options options) {
        KeyMapping[] keys = {
            options.keyAttack, options.keyUse, options.keyPickItem, options.keyDrop, options.keySwapOffhand,
            options.keyUp, options.keyDown, options.keyLeft, options.keyRight, options.keyJump, options.keyShift,
            options.keySprint, options.keyTogglePerspective, options.keySmoothCamera, options.keyFullscreen
        };
        for (KeyMapping key : keys) {
            release(key);
        }
        for (KeyMapping key : options.keyHotbarSlots) {
            release(key);
        }
    }

    private static void release(KeyMapping key) {
        while (key.consumeClick()) {
            // drained
        }
        key.setDown(false);
    }
}
