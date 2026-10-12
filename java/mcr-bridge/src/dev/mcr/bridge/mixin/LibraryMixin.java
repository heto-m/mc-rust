package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.DeviceList;
import com.mojang.blaze3d.audio.DeviceTracker;
import com.mojang.blaze3d.audio.Library;
import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.sound.FixedDeviceTracker;
import dev.mcr.bridge.sound.LibraryPools;
import java.util.OptionalLong;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No OpenAL device: vanilla's channel pools, sized for a 255-source device, run over mcr-audio. */
@Mixin(Library.class)
abstract class LibraryMixin {
    @Shadow
    private String currentDeviceName;

    @Inject(method = "tryOpenDevice", at = @At("HEAD"))
    private static void mcr$countOpen(@Nullable String name, CallbackInfoReturnable<OptionalLong> cir) {
        Bridge.info("openal_open_device", name);
    }

    @Inject(method = "init", at = @At("HEAD"), cancellable = true)
    private void mcr$bridgeInit(
            @Nullable String preferredDevice, DeviceList currentDevices, boolean useHrtf, CallbackInfo ci) {
        LibraryPools.install((Library) (Object) this);
        currentDeviceName = FixedDeviceTracker.DEVICE;
        ci.cancel();
    }

    @Redirect(
            method = "cleanup",
            at = @At(value = "INVOKE", target = "Lorg/lwjgl/openal/ALC10;alcDestroyContext(J)V", remap = false))
    private void mcr$noContext(long context) {}

    @Inject(method = "createDeviceTracker", at = @At("HEAD"), cancellable = true)
    private static void mcr$fixedDevice(CallbackInfoReturnable<DeviceTracker> cir) {
        cir.setReturnValue(new FixedDeviceTracker());
    }
}
