package dev.mcr.testmod.mixin;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.math.Axis;
import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rolls the bob pose at the start of {@code bobHurt}, the way Camera Overhaul does. */
@Mixin(GameRenderer.class)
abstract class PoseRollMixin {
    @Inject(method = "bobHurt", at = @At("HEAD"))
    private void mcr_testmod$roll(CameraRenderState cameraState, PoseStack poseStack, CallbackInfo ci) {
        if (McrTestMod.POSE_ROLL) {
            poseStack.rotateDegrees(Axis.ZP, 10.0F);
        }
    }
}
