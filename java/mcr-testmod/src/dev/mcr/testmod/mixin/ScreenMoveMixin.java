package dev.mcr.testmod.mixin;

import com.mojang.blaze3d.platform.InputConstants;
import dev.mcr.testmod.McrTestMod;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.player.LocalPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** `-Dmcr.testmod.moveInScreens=true`: the movement keys keep working in screens, like InvMove. */
@Mixin(LocalPlayer.class)
abstract class ScreenMoveMixin {
    @Inject(
            method = "aiStep",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/client/player/ClientInput;tick()V"))
    private void mcr_testmod$moveInScreens(CallbackInfo ci) {
        Minecraft minecraft = Minecraft.getInstance();
        if (!McrTestMod.MOVE_IN_SCREENS || minecraft.gui.screen() == null) {
            return;
        }
        Options o = minecraft.options;
        for (KeyMapping key : new KeyMapping[] {o.keyUp, o.keyDown, o.keyLeft, o.keyRight, o.keyJump}) {
            InputConstants.Key bound = InputConstants.getKey(key.saveString());
            key.setDown(bound.getType() == InputConstants.Type.KEYBOARD && InputConstants.isKeyDown(bound.getValue()));
        }
    }
}
