package dev.mcr.bridge.net;

import com.mojang.blaze3d.platform.InputConstants;
import dev.mcr.bridge.McrCameraHandoff;
import dev.mcr.bridge.McrFrame;
import dev.mcr.bridge.mixin.KeyMappingAccessor;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Options;

/**
 * Vanilla gameplay keys belong to Rust; screen keys and mod-registered mappings stay Java's. While a
 * mod camera took the player's movement input, the movement keys follow the keyboard for it. The
 * perspective key is Java's while mcr's camera paces Java, since mcr then follows Java's camera type.
 */
public final class GameplayKeys {
    private GameplayKeys() {}

    public static void suppress(Options options) {
        KeyMapping[] keys = {
            options.keyAttack, options.keyUse, options.keyPickItem, options.keyDrop, options.keySwapOffhand,
            options.keySmoothCamera, options.keyFullscreen
        };
        KeyMapping[] movement = {
            options.keyUp, options.keyDown, options.keyLeft, options.keyRight, options.keyJump, options.keyShift,
            options.keySprint
        };
        for (KeyMapping key : keys) {
            release(key);
        }
        for (KeyMapping key : options.keyHotbarSlots) {
            release(key);
        }
        if (McrFrame.camera() == null) {
            release(options.keyTogglePerspective);
        }
        boolean captured = McrCameraHandoff.inputCaptured();
        for (KeyMapping key : movement) {
            if (captured) {
                mirror(key);
            } else {
                release(key);
            }
        }
    }

    private static void mirror(KeyMapping key) {
        InputConstants.Key bound = ((KeyMappingAccessor) key).mcr$key();
        key.setDown(bound.getType() == InputConstants.Type.KEYBOARD && InputConstants.isKeyDown(bound.getValue()));
    }

    private static void release(KeyMapping key) {
        while (key.consumeClick()) {
            // drained
        }
        key.setDown(false);
    }
}
