package dev.mcr.bridge.mixin;

import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.gpu.FrameSlots;
import dev.mcr.bridge.world.BlockEntitySections;
import dev.mcr.bridge.world.SectionUpdates;
import dev.mcr.bridge.world.WorldStats;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.state.level.LevelRenderState;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Rust owns terrain and the sky: Java's level render draws only features (entities, block entities,
 * particles, outlines), cut into mcr's world slots at vanilla's phase boundaries.
 */
@Mixin(LevelRenderer.class)
abstract class LevelRendererMixin {
    @Shadow @Final private LevelRenderState levelRenderState;
    @Shadow private boolean currentFrameRendersEntityOutline;

    @Inject(method = "render", at = @At("HEAD"))
    private void mcr$worldSlots(CallbackInfo ci) {
        FrameSlots.beginWorld();
        WorldStats.frame(levelRenderState, currentFrameRendersEntityOutline);
    }

    @Inject(method = "executeSolid", at = @At("RETURN"))
    private void mcr$afterSolidFeatures(CallbackInfo ci) {
        FrameSlots.cutTo(BridgeAbi.SLOT_WORLD_TRANSLUCENT);
    }

    @Inject(
            method = "executeClassicTransparency",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/feature/FeatureRenderDispatcher$PreparedFrame;"
                            + "executeTranslucent(Lcom/mojang/renderpearl/api/commands/RenderPass;)V",
                    shift = At.Shift.AFTER))
    private void mcr$afterTranslucentFeatures(CallbackInfo ci) {
        FrameSlots.cutTo(BridgeAbi.SLOT_WORLD_AFTER);
    }

    @Inject(method = "addSkyPass", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsTheSky(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "extractSectionDrawGroups", at = @At("HEAD"), cancellable = true)
    private void mcr$noTerrainDraws(CallbackInfoReturnable<Integer> cir) {
        cir.setReturnValue(0);
    }

    @Inject(method = "clearVisibleSections", at = @At("HEAD"))
    private void mcr$visibleSectionsCleared(CallbackInfo ci) {
        BlockEntitySections.invalidate();
        SectionUpdates.changed();
    }
}
