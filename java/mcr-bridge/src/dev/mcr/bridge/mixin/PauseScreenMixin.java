package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrInput;
import dev.mcr.bridge.McrWorlds;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(PauseScreen.class)
abstract class PauseScreenMixin extends Screen {
    protected PauseScreenMixin(Component title) {
        super(title);
    }

    @Inject(method = "init", at = @At("TAIL"))
    private void mcr$button(CallbackInfo ci) {
        if (McrInput.active() && ((PauseScreen) (Object) this).showsPauseMenu()) {
            addRenderableWidget(McrInput.button());
        }
    }

    @Redirect(method = "createPauseMenu", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Minecraft;isLocalServer()Z"))
    private boolean mcr$saveAndQuit(Minecraft minecraft) {
        return minecraft.isLocalServer() || McrWorlds.local();
    }
}
