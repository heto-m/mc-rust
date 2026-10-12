package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.platform.NativeImage;
import dev.mcr.bridge.gpu.FrameSlots;
import java.util.function.Consumer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A screenshot of the main target taken before the GUI slot waits until the frame is whole. */
@Mixin(Screenshot.class)
abstract class ScreenshotMixin {
    @Inject(
            method = "takeScreenshot(Lcom/mojang/blaze3d/pipeline/RenderTarget;ILjava/util/function/Consumer;)V",
            at = @At("HEAD"),
            cancellable = true)
    private static void mcr$afterGui(RenderTarget target, int downscale, Consumer<NativeImage> callback, CallbackInfo ci) {
        Minecraft minecraft = Minecraft.getInstance();
        if (minecraft.gameRenderer != null
                && target == minecraft.gameRenderer.mainRenderTarget()
                && FrameSlots.deferMainRead(() -> Screenshot.takeScreenshot(target, downscale, callback))) {
            ci.cancel();
        }
    }
}
