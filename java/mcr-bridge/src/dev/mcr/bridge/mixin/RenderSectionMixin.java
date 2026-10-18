package dev.mcr.bridge.mixin;

import dev.mcr.bridge.world.BlockEntitySections;
import dev.mcr.bridge.world.BridgeSectionMesh;
import java.util.List;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Rust meshes terrain: a section "compiles" into its block entity list, on the render thread. */
@Mixin(SectionRenderDispatcher.RenderSection.class)
abstract class RenderSectionMixin {
    @Shadow
    public abstract long getSectionNode();

    @Shadow
    private SectionMesh setSectionMesh(SectionMesh sectionMesh) {
        throw new AssertionError();
    }

    @Shadow
    private void cancelTasks() {}

    private static final String REGION = "Lnet/minecraft/client/renderer/chunk/RenderSectionRegion;";

    @Inject(
            method = {"compileSync(" + REGION + ")V", "compileAsync(" + REGION + ")V"},
            at = @At("HEAD"),
            cancellable = true)
    private void mcr$scan(RenderSectionRegion region, CallbackInfo ci) {
        cancelTasks();
        setSectionMesh(BridgeSectionMesh.scan(getSectionNode()));
        ci.cancel();
    }

    // NeoForge's overloads with extra renderers, which its LevelRenderer calls instead.
    @Inject(
            method = {"compileSync(" + REGION + "Ljava/util/List;)V", "compileAsync(" + REGION + "Ljava/util/List;)V"},
            at = @At("HEAD"),
            cancellable = true,
            require = 0)
    private void mcr$scanWithRenderers(RenderSectionRegion region, List<?> renderers, CallbackInfo ci) {
        mcr$scan(region, ci);
    }

    @Inject(method = "setSectionMesh", at = @At("HEAD"))
    private void mcr$meshReplaced(SectionMesh sectionMesh, CallbackInfoReturnable<SectionMesh> cir) {
        BlockEntitySections.invalidate();
    }

    @Inject(method = "reset", at = @At("HEAD"))
    private void mcr$meshReset(CallbackInfo ci) {
        BlockEntitySections.invalidate();
    }
}
