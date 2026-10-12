package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import net.minecraft.client.DeltaTracker;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** In lockstep Rust's accumulator decides the ticks and the partial tick, not Java's clock. */
@Mixin(DeltaTracker.Timer.class)
abstract class DeltaTrackerTimerMixin {
    @Shadow
    private float deltaTicks;

    @Shadow
    private float deltaTickResidual;

    @Shadow
    private long lastMs;

    @Inject(method = "advanceGameTime", at = @At("HEAD"), cancellable = true)
    private void mcr$rustTicks(long currentMs, CallbackInfoReturnable<Integer> cir) {
        if (!McrFrame.lockstep()) {
            return;
        }
        int ticks = McrFrame.takeTicks();
        float partial = McrFrame.partialTick();
        deltaTicks = Math.max(0.0F, ticks + partial - deltaTickResidual);
        deltaTickResidual = partial;
        lastMs = currentMs;
        cir.setReturnValue(ticks);
    }
}
