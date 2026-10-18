package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.dialog.DialogScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** A custom dialog's leave warning says Save and Quit in mcr's local world. */
@Mixin(DialogScreen.WarningScreen.class)
abstract class DialogWarningScreenMixin {
    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Minecraft;isLocalServer()Z"))
    private static boolean mcr$saveAndQuit(Minecraft minecraft) {
        return minecraft.isLocalServer() || McrWorlds.local();
    }
}
