package dev.mcr.bridge;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import net.minecraft.client.renderer.state.OptionsRenderState;
import net.minecraft.client.renderer.state.level.CameraEntityRenderState;
import net.minecraft.util.Mth;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Quaternionf;

/**
 * The roll mods add to the level's bob pose (Camera Overhaul rolls it in {@code bobHurt}): mcr replaces Java's level
 * projection with its own view, so it rolls that view by the pose's share beyond vanilla's own hurt tilt and bobbing.
 */
public final class McrViewRoll {
    private static final float EPSILON = 1.0E-5F;
    private static final Matrix4f SHARE = new Matrix4f();
    private static final Quaternionf ROTATION = new Quaternionf();
    private static float roll;

    private McrViewRoll() {}

    public static void reset() {
        roll = 0.0F;
    }

    /** {@code pose} is the bob pose {@code renderLevel} multiplies into its projection, after every mod's hook. */
    public static void levelPose(Matrix4fc pose, CameraEntityRenderState entity, OptionsRenderState options) {
        if (McrFrame.camera() == null) {
            roll = 0.0F;
            return;
        }
        Matrix4f vanilla = vanillaPose(entity, options);
        Matrix4f share = SHARE.set(pose).mul(vanilla.invert());
        if (!share.isFinite()) {
            roll = 0.0F;
            return;
        }
        Quaternionf q = share.getNormalizedRotation(ROTATION);
        float sign = q.w < 0.0F ? -1.0F : 1.0F;
        float twist = 2.0F * (float) Math.atan2(sign * q.z, sign * q.w);
        roll = Float.isFinite(twist) && Math.abs(twist) > EPSILON ? twist : 0.0F;
    }

    /** The pose vanilla's {@code bobHurt} and {@code bobView} build from this state when no mod hooks them. */
    private static Matrix4f vanillaPose(CameraEntityRenderState entity, OptionsRenderState options) {
        PoseStack stack = new PoseStack();
        if (entity.isLiving) {
            if (entity.isDeadOrDying) {
                stack.rotateDegrees(Axis.ZP, 40.0F - 8000.0F / (Math.min(entity.deathTime, 20.0F) + 200.0F));
            }
            if (entity.hurtTime >= 0.0F) {
                float t = entity.hurtTime / entity.hurtDuration;
                float shake = Mth.sin(t * t * t * t * (float) Math.PI);
                stack.rotateDegrees(Axis.YP, -entity.hurtDir);
                stack.rotateDegrees(Axis.ZP, (float) (-shake * 14.0 * options.damageTiltStrength));
                stack.rotateDegrees(Axis.YP, entity.hurtDir);
            }
        }
        if (options.bobView && entity.isPlayer) {
            float walk = entity.backwardsInterpolatedWalkDistance * (float) Math.PI;
            stack.translate(Mth.sin(walk) * entity.bob * 0.5F, -Math.abs(Mth.cos(walk) * entity.bob), 0.0F);
            stack.rotateDegrees(Axis.ZP, Mth.sin(walk) * entity.bob * 3.0F);
            stack.rotateDegrees(Axis.XP, Math.abs(Mth.cos(walk - 0.2F) * entity.bob) * 5.0F);
        }
        return stack.last().pose();
    }

    /** Radians in mcr's view space: a pose roll and mcr's view roll share their sign. */
    static float roll() {
        return roll;
    }
}
