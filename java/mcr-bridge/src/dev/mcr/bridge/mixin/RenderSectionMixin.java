package dev.mcr.bridge.mixin;

import dev.mcr.bridge.world.BridgeSectionMesh;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

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

    @Inject(method = "compileSync", at = @At("HEAD"), cancellable = true)
    private void mcr$scanSync(RenderSectionRegion region, CallbackInfo ci) {
        cancelTasks();
        setSectionMesh(BridgeSectionMesh.scan(getSectionNode()));
        ci.cancel();
    }

    @Inject(method = "compileAsync", at = @At("HEAD"), cancellable = true)
    private void mcr$scanAsync(RenderSectionRegion region, CallbackInfo ci) {
        cancelTasks();
        setSectionMesh(BridgeSectionMesh.scan(getSectionNode()));
        ci.cancel();
    }
}
