package dev.mcr.bridge.mixin;

import dev.mcr.bridge.gpu.FrameSlots;
import net.minecraft.client.gui.render.GuiRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(GuiRenderer.class)
abstract class GuiRendererMixin {
    @Inject(method = "render", at = @At("HEAD"))
    private void mcr$guiSlot(CallbackInfo ci) {
        FrameSlots.beginGui();
    }

    @Inject(method = "render", at = @At("RETURN"))
    private void mcr$endGuiSlot(CallbackInfo ci) {
        FrameSlots.endGui();
    }
}
