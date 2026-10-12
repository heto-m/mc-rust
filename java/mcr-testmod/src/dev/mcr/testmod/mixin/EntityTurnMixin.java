package dev.mcr.testmod.mixin;

import dev.mcr.testmod.TestCamera;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The mouse turns the free camera instead of the player, like Freecam. */
@Mixin(Entity.class)
abstract class EntityTurnMixin {
    @Inject(method = "turn", at = @At("HEAD"), cancellable = true)
    private void mcr_testmod$turnCamera(double xo, double yo, CallbackInfo ci) {
        Entity camera = TestCamera.free();
        if (camera != null && (Object) this == Minecraft.getInstance().player) {
            camera.turn(xo, yo);
            ci.cancel();
        }
    }
}
