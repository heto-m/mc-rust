package dev.mcr.bridge.mixin;

import dev.mcr.bridge.world.SectionUpdates;
import net.minecraft.client.SectionUpdateTracker;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Tells {@link SectionUpdates} when vanilla's per-frame dirty walk has something to find. */
@Mixin(SectionUpdateTracker.SectionDirtyState.class)
abstract class SectionDirtyStateMixin {
    @Inject(method = "setDirty", at = @At("HEAD"))
    private void mcr$dirtied(boolean fromPlayer, CallbackInfo ci) {
        SectionUpdates.changed();
    }

    @Inject(method = "setSectionNode", at = @At("HEAD"))
    private void mcr$moved(long sectionNode, CallbackInfo ci) {
        SectionUpdates.changed();
    }
}
