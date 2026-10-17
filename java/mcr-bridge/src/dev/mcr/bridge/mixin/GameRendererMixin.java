package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrCamera;
import dev.mcr.bridge.McrCameraHandoff;
import dev.mcr.bridge.McrFrame;
import dev.mcr.bridge.McrFrameTimes;
import dev.mcr.bridge.McrParticles;
import dev.mcr.bridge.gpu.FrameSlots;
import net.minecraft.client.Camera;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.Projection;
import net.minecraft.client.renderer.ProjectionMatrixBuffer;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.state.GameRenderState;
import org.joml.Matrix4f;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Java renders the world's features with mcr's camera, straight into mcr's scene. */
@Mixin(GameRenderer.class)
abstract class GameRendererMixin {
    @Shadow
    @Final
    private GameRenderState gameRenderState;

    @Shadow
    @Final
    private Camera mainCamera;

    @Inject(method = "extractCamera", at = @At("HEAD"))
    private void mcr$cameraHandoff(CallbackInfo ci) {
        McrCameraHandoff.extract(mainCamera);
    }

    @ModifyArg(
            method = "renderLevel",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/ProjectionMatrixBuffer;getBuffer(Lorg/joml/Matrix4f;)"
                            + "Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;"))
    private Matrix4f mcr$rustProjection(Matrix4f vanilla) {
        McrCamera camera = McrFrame.camera();
        if (camera != null) {
            return camera.levelProjection(gameRenderState.levelRenderState.cameraRenderState.viewRotationMatrix);
        }
        return FrameSlots.inWorld() ? McrCamera.flipped(vanilla) : vanilla;
    }

    @Inject(method = "extract", at = @At("HEAD"))
    private void mcr$extractStart(CallbackInfo ci) {
        McrFrameTimes.begin(BridgeAbi.PROF_EXTRACT);
    }

    @Inject(method = "extract", at = @At("RETURN"))
    private void mcr$extractEnd(CallbackInfo ci) {
        McrFrameTimes.end(BridgeAbi.PROF_EXTRACT);
    }

    @Inject(method = "render", at = @At("HEAD"))
    private void mcr$renderStart(CallbackInfo ci) {
        McrFrameTimes.begin(BridgeAbi.PROF_RENDER);
    }

    @Inject(method = "render", at = @At("RETURN"))
    private void mcr$renderEnd(CallbackInfo ci) {
        McrFrameTimes.end(BridgeAbi.PROF_RENDER);
    }

    @Inject(method = "renderLevel", at = @At("HEAD"))
    private void mcr$worldStart(CallbackInfo ci) {
        McrFrameTimes.begin(BridgeAbi.PROF_WORLD);
    }

    @Inject(method = "renderLevel", at = @At("RETURN"))
    private void mcr$worldEnd(CallbackInfo ci) {
        McrFrameTimes.end(BridgeAbi.PROF_WORLD);
    }

    @Inject(
            method = "renderLevel",
            at = @At(
                    value = "INVOKE",
                    target = "Lcom/mojang/blaze3d/systems/RenderSystem;setProjectionMatrix("
                            + "Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;Lcom/mojang/blaze3d/ProjectionType;)V",
                    shift = At.Shift.AFTER))
    private void mcr$levelView(CallbackInfo ci) {
        GpuBufferSlice projection = RenderSystem.getProjectionMatrixBuffer();
        if (McrFrame.camera() == null || projection == null) {
            return;
        }
        CameraRenderState camera = gameRenderState.levelRenderState.cameraRenderState;
        FrameSlots.levelView(projection, camera.pos.x, camera.pos.y, camera.pos.z, camera.viewRotationMatrix);
    }

    @Inject(method = "render3dHud", at = @At("HEAD"))
    private void mcr$postWorldSlot(CallbackInfo ci) {
        FrameSlots.cutTo(BridgeAbi.SLOT_POST_WORLD);
    }

    @Redirect(
            method = "render3dHud",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/ProjectionMatrixBuffer;getBuffer("
                            + "Lnet/minecraft/client/renderer/Projection;)Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;"))
    private GpuBufferSlice mcr$topDownHand(ProjectionMatrixBuffer buffer, Projection projection) {
        if (!FrameSlots.inWorld()) {
            return buffer.getBuffer(projection);
        }
        return buffer.getBuffer(McrCamera.flipped(projection.getMatrix(new Matrix4f())));
    }

    @Inject(method = "useImprovedTransparency", at = @At("HEAD"), cancellable = true)
    private void mcr$classicTransparency(CallbackInfoReturnable<Boolean> cir) {
        if (McrFrame.COMPOSE) {
            cir.setReturnValue(false);
        }
    }

    @Inject(method = "tick", at = @At("HEAD"))
    private void mcr$tickStart(CallbackInfo ci) {
        McrParticles.enter();
    }

    @Inject(method = "tick", at = @At("RETURN"))
    private void mcr$tickEnd(CallbackInfo ci) {
        McrParticles.exit();
    }
}
