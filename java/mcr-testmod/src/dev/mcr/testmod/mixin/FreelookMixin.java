package dev.mcr.testmod.mixin;

import dev.mcr.testmod.Freelook;
import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * `-Dmcr.testmod.freelook=true`: the mouse turns only the camera, hooked inside {@code alignWithEntity} like
 * Freelook does; the player keeps its rotation.
 */
@Mixin(Camera.class)
abstract class FreelookMixin {
    @Shadow
    private Entity entity;

    @Shadow
    protected abstract void setRotation(float yRot, float xRot);

    @Inject(
            method = "alignWithEntity",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;setRotation(FF)V", ordinal = 1, shift = At.Shift.AFTER))
    private void mcr_testmod$freelook(float partialTicks, CallbackInfo ci) {
        if (McrTestMod.FREELOOK && entity == Minecraft.getInstance().player) {
            setRotation(entity.getYRot() + Freelook.yaw, Math.clamp(entity.getXRot() + Freelook.pitch, -90.0F, 90.0F));
        }
    }
}
