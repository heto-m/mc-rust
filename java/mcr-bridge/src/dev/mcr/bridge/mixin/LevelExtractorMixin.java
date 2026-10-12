package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrSections;
import dev.mcr.bridge.McrWeather;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.chunk.RenderRegionCache;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.client.renderer.extract.LevelExtractor;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Rust draws the block outline and destroy overlay, sections compile without a block copy, and
 * entities and block entities in sections mcr's visibility graph ruled out are not extracted.
 */
@Mixin(LevelExtractor.class)
abstract class LevelExtractorMixin {
    @Shadow
    @Final
    private Minecraft minecraft;

    @Shadow
    @Final
    private LevelRenderer levelRenderer;

    @Inject(
            method = "extract",
            at = @At(
                    value = "INVOKE_STRING",
                    target = "Lnet/minecraft/util/profiling/ProfilerFiller;popPush(Ljava/lang/String;)V",
                    args = "ldc=weather"))
    private void mcr$weatherStep(CallbackInfo ci) {
        McrWeather.step();
    }

    @Inject(method = "extractVisibleBlockEntities", at = @At("HEAD"))
    private void mcr$blockEntitiesStart(CallbackInfo ci) {
        McrSections.blockEntities(true);
    }

    @Inject(method = "extractVisibleBlockEntities", at = @At("RETURN"))
    private void mcr$blockEntitiesEnd(CallbackInfo ci) {
        McrSections.blockEntities(false);
    }

    @Inject(method = "isEntityVisible", at = @At("RETURN"), cancellable = true)
    private void mcr$sectionCulling(
            Entity entity, Frustum frustum, double camX, double camY, double camZ, float partialTicks, long fade,
            CallbackInfoReturnable<Boolean> cir) {
        if (!cir.getReturnValueZ()
                || entity == minecraft.player
                || entity == minecraft.getCameraEntity()
                || minecraft.player != null && entity.hasIndirectPassenger(minecraft.player)) {
            return;
        }
        EntityRenderer<?, ?> renderer = levelRenderer.entityRenderDispatcher().getRenderer(entity);
        if (((EntityRendererInvoker) renderer).mcr$affectedByCulling(entity) && !McrSections.entityVisible(entity)) {
            cir.setReturnValue(false);
        }
    }
    @Redirect(
            method = "extract",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/client/renderer/chunk/RenderRegionCache;createRegion("
                            + "Lnet/minecraft/client/multiplayer/ClientLevel;J)"
                            + "Lnet/minecraft/client/renderer/chunk/RenderSectionRegion;"))
    private RenderSectionRegion mcr$noSectionCopy(RenderRegionCache cache, ClientLevel level, long sectionNode) {
        return null;
    }

    @Inject(method = "extractBlockOutline", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsTheOutline(CallbackInfo ci) {
        ci.cancel();
    }

    @Inject(method = "extractBlockDestroyAnimation", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsDestroyStages(CallbackInfo ci) {
        ci.cancel();
    }
}
