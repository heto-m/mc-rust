package dev.mcr.testmod.mixin;

import dev.mcr.bridge.Bridge;
import net.minecraft.client.player.ClientInput;
import net.minecraft.client.tutorial.MovementTutorialStepInstance;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Reports `tutorial_moved` once the movement tutorial saw a movement key. */
@Mixin(MovementTutorialStepInstance.class)
abstract class TutorialMoveMixin {
    @Shadow
    private boolean moved;

    @Unique
    private static boolean mcr_testmod$reported;

    @Inject(method = "onInput", at = @At("TAIL"))
    private void mcr_testmod$reportMoved(ClientInput input, CallbackInfo ci) {
        if (moved && !mcr_testmod$reported) {
            mcr_testmod$reported = true;
            Bridge.info("tutorial_moved", "1");
        }
    }
}
