package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrInput;
import dev.mcr.bridge.McrPreload;
import dev.mcr.bridge.McrRegistries;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(TitleScreen.class)
abstract class TitleScreenMixin extends Screen {
    protected TitleScreenMixin(Component title) {
        super(title);
    }

    @Inject(method = "init", at = @At("TAIL"))
    private void mcr$button(CallbackInfo ci) {
        McrPreload.milestone(McrPreload.TITLE);
        McrRegistries.reportAtTitle();
        if (McrInput.active()) {
            addRenderableWidget(McrInput.button());
        }
    }
}
