package dev.mcr.bridge.mixin;

import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrInput;
import com.mojang.realmsclient.RealmsMainScreen;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.client.Minecraft;
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
    private static final SystemToast.SystemToastId NOT_SUPPORTED = new SystemToast.SystemToastId();

    @Shadow
    public abstract @Nullable Screen screen();

    @Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)
    private void mcr$notYet(@Nullable Screen requested, CallbackInfo ci) {
        if (McrInput.active() && (requested instanceof SelectWorldScreen || requested instanceof RealmsMainScreen)) {
            Minecraft minecraft = Minecraft.getInstance();
            SystemToast.addOrUpdate(
                    minecraft.gui.toastManager(),
                    NOT_SUPPORTED,
                    Component.literal(requested instanceof SelectWorldScreen ? "Singleplayer" : "Realms"),
                    Component.literal("Not supported in mcr yet"));
            ci.cancel();
        }
    }

    @Inject(method = "setScreen", at = @At("TAIL"))
    private void mcr$reportScreen(@Nullable Screen requested, CallbackInfo ci) {
        if (McrInput.active()) {
            McrInput.publish(Minecraft.getInstance());
        }
        Screen screen = screen();
        if (screen == null) {
            Bridge.report(BridgeAbi.REPORT_SCREEN, "");
            return;
        }
        Bridge.report(BridgeAbi.REPORT_SCREEN, screen.getClass().getName());
        McrInput.probeEditBoxes(screen);
        StringBuilder labels = new StringBuilder();
        StringBuilder layout = new StringBuilder().append(Minecraft.getInstance().getWindow().getGuiScale());
        for (GuiEventListener child : screen.children()) {
            if (child instanceof AbstractWidget widget) {
                if (!labels.isEmpty()) {
                    labels.append('|');
                }
                String label = widget.getMessage().getString();
                labels.append(label);
                layout.append('|').append(widget.getX()).append(',').append(widget.getY()).append(',')
                        .append(widget.getWidth()).append(',').append(widget.getHeight()).append(',').append(label);
            }
        }
        Bridge.info("screen_widgets", labels);
        Bridge.info("screen_layout", layout);
        Runtime rt = Runtime.getRuntime();
        Bridge.info("heap_mib", ((rt.totalMemory() - rt.freeMemory()) >> 20) + "/" + (rt.totalMemory() >> 20) + "/" + (rt.maxMemory() >> 20));
    }
}
