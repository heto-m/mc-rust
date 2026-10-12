package dev.mcr.bridge.mixin;

import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Proxy;
import net.minecraft.client.main.GameConfig;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Skin, profile and pack downloads take Minecraft's own proxy, not the JVM's: point it at mcr's loopback proxy. */
@Mixin(GameConfig.UserData.class)
abstract class GameConfigUserDataMixin {
    @Shadow @Final @Mutable public Proxy proxy;

    @Inject(method = "<init>", at = @At("RETURN"))
    private void mcr$throughRust(CallbackInfo ci) {
        Integer port = Integer.getInteger("https.proxyPort");
        if (port != null && port > 0 && port < 65536 && Proxy.NO_PROXY.equals(proxy)) {
            proxy = new Proxy(Proxy.Type.HTTP, new InetSocketAddress(InetAddress.getLoopbackAddress(), port));
        }
    }
}
