package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.DeviceList;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(DeviceList.class)
abstract class DeviceListMixin {
    @Inject(method = "query", at = @At("HEAD"), cancellable = true)
    private static void mcr$noEnumeration(CallbackInfoReturnable<DeviceList> cir) {
        cir.setReturnValue(DeviceList.EMPTY);
    }
}
