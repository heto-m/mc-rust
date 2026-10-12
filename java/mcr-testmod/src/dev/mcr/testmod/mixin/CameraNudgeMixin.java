package dev.mcr.testmod.mixin;

import dev.mcr.testmod.TestCamera;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Lifts the camera above the player in the nudge mode, like a camera tweak mod's mixin. */
@Mixin(Camera.class)
abstract class CameraNudgeMixin {
    @Shadow
    private Vec3 position;

    @Shadow
    protected abstract void setPosition(double x, double y, double z);

    @Inject(method = "update", at = @At("TAIL"))
    private void mcr_testmod$nudge(DeltaTracker deltaTracker, CallbackInfo ci) {
        if (TestCamera.mode() == TestCamera.Mode.NUDGE) {
            setPosition(position.x, position.y + TestCamera.NUDGE, position.z);
        }
    }
}
