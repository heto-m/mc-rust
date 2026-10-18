package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import net.minecraft.server.permissions.Permission;
import net.minecraft.server.permissions.PermissionSet;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** The player owns mcr's local world like an integrated server's owner, so World Options' difficulty is theirs. */
@Mixin(targets = "net.minecraft.client.gui.screens.WorldOptionsScreen$DifficultyButtons")
abstract class DifficultyButtonsMixin {
    @Redirect(
            method = "updateDifficultyButtonsState",
            at = @At(
                    value = "INVOKE",
                    target = "Lnet/minecraft/server/permissions/PermissionSet;hasPermission(Lnet/minecraft/server/permissions/Permission;)Z"))
    private static boolean mcr$owner(PermissionSet permissions, Permission permission) {
        return permissions.hasPermission(permission) || McrWorlds.local();
    }
}
