package dev.mcr.bridge.mixin;

import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.BridgeAbi;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Gui.class)
abstract class GuiMixin {
    @Shadow
    public abstract @Nullable Screen screen();

    @Inject(method = "setScreen", at = @At("TAIL"))
    private void mcr$reportScreen(@Nullable Screen requested, CallbackInfo ci) {
        Screen screen = screen();
        if (screen == null) {
            Bridge.report(BridgeAbi.REPORT_SCREEN, "");
            return;
        }
        Bridge.report(BridgeAbi.REPORT_SCREEN, screen.getClass().getName());
        StringBuilder labels = new StringBuilder();
        for (GuiEventListener child : screen.children()) {
            if (child instanceof AbstractWidget widget) {
                if (!labels.isEmpty()) {
                    labels.append('|');
                }
                labels.append(widget.getMessage().getString());
            }
        }
        Bridge.info("screen_widgets", labels);
        Runtime rt = Runtime.getRuntime();
        Bridge.info("heap_mib", ((rt.totalMemory() - rt.freeMemory()) >> 20) + "/" + (rt.totalMemory() >> 20) + "/" + (rt.maxMemory() >> 20));
    }
}
