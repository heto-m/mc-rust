package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.systems.RenderSystem;
import dev.mcr.bridge.McrCamera;
import dev.mcr.bridge.McrFrame;
import net.minecraft.client.Camera;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import org.joml.Matrix4f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** mcr owns the camera: Java's eye, rotation, perspective and culling projection are Rust's. */
@Mixin(Camera.class)
abstract class CameraMixin {
    @Shadow
    private boolean detached;

    @Shadow
    private float depthFar;

    @Shadow
    protected abstract void setRotation(float yRot, float xRot);

    @Shadow
    protected abstract void setPosition(double x, double y, double z);

    @Inject(method = "alignWithEntity", at = @At("HEAD"), cancellable = true)
    private void mcr$rustCamera(float partialTicks, CallbackInfo ci) {
        McrCamera camera = McrFrame.camera();
        if (camera == null) {
            return;
        }
        setRotation(camera.yRot(), camera.xRot());
        setPosition(camera.x(), camera.y(), camera.z());
        detached = camera.detached();
        Options options = Minecraft.getInstance().options;
        if (options.getCameraType().isFirstPerson() == detached) {
            options.setCameraType(detached ? CameraType.THIRD_PERSON_BACK : CameraType.FIRST_PERSON);
        }
        ci.cancel();
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
}
