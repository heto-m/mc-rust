package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;
import java.lang.ref.WeakReference;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.KeyboardInput;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.jspecify.annotations.Nullable;

/**
 * Mods that move Java's camera (Freecam, camera tweaks) hand it to mcr: Java reports the final
 * camera each frame it differs from mcr's player camera, and renders from the camera mcr renders.
 */
public final class McrCameraHandoff {
    private static final VarHandle STATE_LONG = ValueLayout.JAVA_LONG.varHandle();
    private static final VarHandle STATE_INT = ValueLayout.JAVA_INT.varHandle();
    private static final double POS_EPSILON = 1.0E-4;
    private static final float ROT_EPSILON = 1.0E-3F;

    /** Placement of Java's camera, implemented by the camera mixin. */
    public interface Placement {
        /** {@code roll} in radians about the view axis, in vanilla's quaternion convention. */
        void mcr$place(double x, double y, double z, float yRot, float xRot, float roll);
    }

    /** The camera entity the server set (spectating); mcr follows it itself, so it is no mod's. Weak, so a left world is not kept alive. */
    private static WeakReference<Entity> serverCamera = new WeakReference<>(null);

    private static int flags;
    private static double x;
    private static double y;
    private static double z;
    private static float yaw;
    private static float pitch;
    /** Roll in vanilla's convention reported this frame; mcr renders it the frame after. */
    private static float roll;
    private static final Quaternionf RELATIVE = new Quaternionf();

    private McrCameraHandoff() {}

    public static void serverCamera(@Nullable Entity entity) {
        serverCamera = new WeakReference<>(entity);
    }

    /** Whether {@code entity} is a camera entity mcr's own camera follows. */
    public static boolean followsRust(@Nullable Entity entity) {
        return entity == null || entity == Minecraft.getInstance().player || entity == serverCamera.get();
    }

    /** The mod took the player's movement keys for its camera. */
    public static boolean inputCaptured() {
        LocalPlayer player = Minecraft.getInstance().player;
        return McrFrame.camera() != null && player != null && player.input != null && !(player.input instanceof KeyboardInput);
    }

    /** Runs after mods placed Java's camera and before the level reads it. */
    public static void extract(Camera camera) {
        McrCamera player = McrFrame.camera();
        flags = 0;
        if (player == null) {
            return;
        }
        Vec3 pos = camera.position();
        float cameraRoll = roll(camera);
        boolean foreign = !followsRust(camera.entity());
        boolean moved = foreign
                || Math.abs(pos.x - player.x()) > POS_EPSILON
                || Math.abs(pos.y - player.y()) > POS_EPSILON
                || Math.abs(pos.z - player.z()) > POS_EPSILON
                || Math.abs(camera.yRot() - player.yRot()) > ROT_EPSILON
                || Math.abs(camera.xRot() - player.xRot()) > ROT_EPSILON
                || Math.abs(cameraRoll) > ROT_EPSILON
                || camera.isDetached() != player.detached();
        boolean finite = Double.isFinite(pos.x) && Double.isFinite(pos.y) && Double.isFinite(pos.z)
                && Float.isFinite(camera.yRot()) && Float.isFinite(camera.xRot()) && Float.isFinite(cameraRoll);
        if (moved && finite) {
            flags = BridgeAbi.CAMERA_MOD
                    | (camera.isDetached() || foreign ? BridgeAbi.CAMERA_DETACHED : 0)
                    | (inputCaptured() ? BridgeAbi.CAMERA_INPUT : 0);
            x = pos.x;
            y = pos.y;
            z = pos.z;
            yaw = McrCamera.toYaw(camera.yRot());
            pitch = McrCamera.toPitch(camera.xRot());
        }
        McrCamera render = McrFrame.renderCamera();
        McrCamera target = render != null ? render : player;
        float targetRoll = render != null ? roll : 0.0F;
        roll = moved && finite ? cameraRoll : 0.0F;
        if (moved || render != null) {
            ((Placement) camera).mcr$place(target.x(), target.y(), target.z(), target.yRot(), target.xRot(), targetRoll);
        }
    }

    /** The camera's turn about its view axis beyond its yaw and pitch, robust at a vertical pitch. */
    private static float roll(Camera camera) {
        float deg = (float) (Math.PI / 180.0);
        Quaternionf q = RELATIVE.rotationYXZ((float) Math.PI - camera.yRot() * deg, -camera.xRot() * deg, 0.0F)
                .conjugate()
                .mul(camera.rotation());
        float sign = q.w < 0.0F ? -1.0F : 1.0F;
        return 2.0F * (float) Math.atan2(sign * q.z, sign * q.w);
    }

    static void publish(MemorySegment state) {
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_POS, Double.doubleToRawLongBits(x));
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_POS + 8, Double.doubleToRawLongBits(y));
        STATE_LONG.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_POS + 16, Double.doubleToRawLongBits(z));
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_YAW, Float.floatToRawIntBits(yaw));
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_PITCH, Float.floatToRawIntBits(pitch));
        // mcr rolls its view matrix, the inverse of the camera's rotation.
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA_ROLL, Float.floatToRawIntBits(-roll));
        STATE_INT.setRelease(state, (long) BridgeAbi.FSTATE_CAMERA, flags);
        flags = 0;
    }

}
