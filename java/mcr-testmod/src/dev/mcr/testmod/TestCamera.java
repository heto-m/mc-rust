package dev.mcr.testmod;

import dev.mcr.bridge.Bridge;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.player.ClientInput;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.decoration.ArmorStand;
import org.jspecify.annotations.Nullable;

/**
 * Camera mods in miniature, cycled by the camera key: a nudge that lifts the camera three blocks
 * (like a camera tweak mixin), then a free camera entity flown with the movement keys while the
 * player stands still (like Freecam), then off.
 */
public final class TestCamera {
    public static final double NUDGE = 3.0;
    private static final double SPEED = 0.5;

    public enum Mode { OFF, NUDGE, FREE }

    private static Mode mode = Mode.OFF;
    private static @Nullable Entity free;
    private static @Nullable ClientInput playerInput;

    private TestCamera() {}

    public static Mode mode() {
        return mode;
    }

    public static @Nullable Entity free() {
        return free;
    }

    static void cycle(Minecraft client) {
        mode = Mode.values()[(mode.ordinal() + 1) % Mode.values().length];
        if (mode == Mode.FREE) {
            enterFree(client);
        } else {
            leaveFree(client);
        }
        Bridge.info("testmod_camera", mode.name());
    }

    private static void enterFree(Minecraft client) {
        LocalPlayer player = client.player;
        if (player == null || client.level == null) {
            return;
        }
        ArmorStand camera = new ArmorStand(client.level, player.getX(), player.getY(), player.getZ());
        camera.setYRot(player.getYRot());
        camera.setXRot(player.getXRot());
        camera.setOldPosAndRot();
        free = camera;
        playerInput = player.input;
        player.input = new ClientInput();
        client.setCameraEntity(camera);
    }

    private static void leaveFree(Minecraft client) {
        LocalPlayer player = client.player;
        if (free != null && player != null) {
            client.setCameraEntity(player);
            if (playerInput != null) {
                player.input = playerInput;
            }
        }
        free = null;
        playerInput = null;
    }

    static void tick(Minecraft client) {
        Entity camera = free;
        if (camera == null) {
            return;
        }
        camera.setOldPosAndRot();
        Options o = client.options;
        double forward = (o.keyUp.isDown() ? 1 : 0) - (o.keyDown.isDown() ? 1 : 0);
        double strafe = (o.keyLeft.isDown() ? 1 : 0) - (o.keyRight.isDown() ? 1 : 0);
        double up = (o.keyJump.isDown() ? 1 : 0) - (o.keyShift.isDown() ? 1 : 0);
        double yaw = Math.toRadians(camera.getYRot());
        double dx = (-Math.sin(yaw) * forward + Math.cos(yaw) * strafe) * SPEED;
        double dz = (Math.cos(yaw) * forward + Math.sin(yaw) * strafe) * SPEED;
        camera.setPos(camera.getX() + dx, camera.getY() + up * SPEED, camera.getZ() + dz);
    }
}
