package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.systems.RenderSystem;
import dev.mcr.bridge.McrCamera;
import dev.mcr.bridge.McrCameraHandoff;
import dev.mcr.bridge.McrFrame;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.ModifyArgs;
import org.spongepowered.asm.mixin.injection.invoke.arg.Args;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Java's eye, rotation and culling projection are mcr's camera; Java's camera type leads the perspective. */
@Mixin(Camera.class)
abstract class CameraMixin implements McrCameraHandoff.Placement {
    @Shadow
    private Entity entity;

    @Shadow
    private Vec3 position;

    @Shadow
    @org.spongepowered.asm.mixin.Final
    private Matrix4f cachedViewRotMatrix;

    @Shadow
    public abstract Matrix4f getViewRotationMatrix(Matrix4f dest);

    @Shadow
    private Matrix4f createProjectionMatrixForCulling() {
        throw new AssertionError();
    }

    @Shadow
    private void prepareCullFrustum(Matrix4fc modelViewMatrix, Matrix4f projectionMatrixForCulling, Vec3 cameraPos) {
        throw new AssertionError();
    }
    @Shadow
    private boolean detached;

    @Shadow
    private float depthFar;

    @Shadow
    private float fov;

    @Shadow
    private float fovModifier;

    @Shadow
    private float oldFovModifier;

    @Unique
    private float mcr$fovPartialTicks;

    @Shadow
    protected abstract void setRotation(float yRot, float xRot);

    @Shadow
    private float modifyFovBasedOnDeathOrFluid(float partialTicks, float fov) {
        throw new AssertionError();
    }

    @Shadow
    protected abstract void setPosition(double x, double y, double z);

    /** Rust's camera while vanilla's alignment runs for the player; mods hooked into it still act on top. */
    @Unique
    private McrCamera mcr$rust;

    @Inject(method = "alignWithEntity", at = @At("HEAD"))
    private void mcr$rustCamera(float partialTicks, CallbackInfo ci) {
        McrCamera camera = McrFrame.camera();
        // Java switched its camera type before mcr followed: vanilla aligns this frame and the handoff carries it.
        boolean samePerspective = camera != null
                && Minecraft.getInstance().options.getCameraType().isFirstPerson() != camera.detached();
        mcr$rust = samePerspective && McrCameraHandoff.followsRust(entity) ? camera : null;
    }

    @Inject(method = "alignWithEntity", at = @At("RETURN"))
    private void mcr$rustCameraDone(float partialTicks, CallbackInfo ci) {
        mcr$rust = null;
        mcr$rustEye = null;
    }

    // Arguments, not redirects: mods inject at these same calls (Freelook, Shoulder Surfing).
    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V"), index = 0, require = 0)
    private float mcr$rustYRot(float yRot) {
        return mcr$rust != null ? mcr$rust.yRot() : yRot;
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V"), index = 1, require = 0)
    private float mcr$rustXRot(float xRot) {
        return mcr$rust != null ? mcr$rust.xRot() : xRot;
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setPosition(DDD)V"), index = 0, require = 0)
    private double mcr$rustX(double x) {
        return mcr$rust != null ? mcr$eye()[0] : x;
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setPosition(DDD)V"), index = 1, require = 0)
    private double mcr$rustY(double y) {
        return mcr$rust != null ? mcr$eye()[1] : y;
    }

    @ModifyArg(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setPosition(DDD)V"), index = 2, require = 0)
    private double mcr$rustZ(double z) {
        return mcr$rust != null ? mcr$eye()[2] : z;
    }

    @ModifyArg(
            method = "alignWithEntity",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setPosition(Lnet/minecraft/world/phys/Vec3;)V"),
            require = 0)
    private Vec3 mcr$rustPosition(Vec3 pos) {
        if (mcr$rust == null) {
            return pos;
        }
        double[] eye = mcr$eye();
        return new Vec3(eye[0], eye[1], eye[2]);
    }

    /** Rust's eye in third person, so the pull-back below (or a mod's own, as Shoulder Surfing's) starts there. */
    @Unique
    private double[] mcr$eye() {
        if (mcr$rustEye == null) {
            mcr$rustEye = mcr$rust.distance() > 0.0F
                    ? mcr$rust.eye()
                    : new double[] {mcr$rust.x(), mcr$rust.y(), mcr$rust.z()};
        }
        return mcr$rustEye;
    }

    @Unique
    private double[] mcr$rustEye;

    /** Vanilla pulls the third-person camera back by Rust's distance; Rust's camera already has the sleeping offset. */
    @ModifyArgs(method = "alignWithEntity", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;move(FFF)V"), require = 0)
    private void mcr$rustOffsets(Args args) {
        if (mcr$rust != null) {
            args.set(0, -mcr$rust.distance());
            args.set(1, 0.0F);
            args.set(2, 0.0F);
        }
    }

    @Shadow
    @org.spongepowered.asm.mixin.Final
    private Quaternionf rotation;

    @Shadow
    @org.spongepowered.asm.mixin.Final
    private Vector3f up;

    @Shadow
    @org.spongepowered.asm.mixin.Final
    private Vector3f left;

    @Override
    public void mcr$place(double x, double y, double z, float yRot, float xRot, float roll) {
        setRotation(yRot, xRot);
        if (roll != 0.0F) {
            rotation.rotateZ(roll);
            new Vector3f(0.0F, 1.0F, 0.0F).rotate(rotation, up);
            new Vector3f(-1.0F, 0.0F, 0.0F).rotate(rotation, left);
        }
        setPosition(x, y, z);
        prepareCullFrustum(getViewRotationMatrix(cachedViewRotMatrix), createProjectionMatrixForCulling(), position);
    }

    @Override
    public void mcr$setDetached(boolean detached) {
        this.detached = detached;
    }

    @Inject(method = "createProjectionMatrixForCulling", at = @At("HEAD"), cancellable = true)
    private void mcr$rustCulling(CallbackInfoReturnable<Matrix4f> cir) {
        McrCamera camera = McrFrame.camera();
        Matrix4f projection = camera == null
                ? null
                : camera.cullProjection(0.05F, depthFar, RenderSystem.getDevice().getDeviceInfo().isZZeroToOne());
        if (projection != null) {
            cir.setReturnValue(projection);
        }
    }

    @Inject(method = "calculateFov", at = @At("HEAD"))
    private void mcr$fovPartialTicks(float partialTicks, CallbackInfoReturnable<Float> cir) {
        mcr$fovPartialTicks = partialTicks;
    }

    /** Rust draws the world with its own FOV, so it scales that by what mods (zoom) did to Java's. */
    @Inject(method = "update", at = @At("TAIL"))
    private void mcr$reportModFov(DeltaTracker deltaTracker, CallbackInfo ci) {
        if (McrFrame.camera() == null) {
            McrFrame.fovScale = 1.0F;
            return;
        }
        float partial = mcr$fovPartialTicks;
        float base = Minecraft.getInstance().options.fov().get().intValue() * Mth.lerp(partial, oldFovModifier, fovModifier);
        float vanilla = modifyFovBasedOnDeathOrFluid(partial, base);
        float scale = fov / vanilla;
        McrFrame.fovScale = vanilla > 0.0F && Float.isFinite(scale) ? scale : 1.0F;
    }
}
