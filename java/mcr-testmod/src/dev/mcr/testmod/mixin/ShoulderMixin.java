package dev.mcr.testmod.mixin;

import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.Camera;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** Redirects the third-person pull-back the way Shoulder Surfing does, adding a sideways offset. */
@Mixin(Camera.class)
abstract class ShoulderMixin {
    @Shadow
    protected abstract void move(float forwards, float up, float right);

    @Redirect(
            method = "alignWithEntity",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;move(FFF)V", ordinal = 0))
    private void mcr_testmod$shoulder(Camera camera, float forwards, float up, float right) {
        move(forwards, up, McrTestMod.SHOULDER ? right - 1.0F : right);
    }
}
