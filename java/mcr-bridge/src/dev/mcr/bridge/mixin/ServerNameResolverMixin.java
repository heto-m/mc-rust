package dev.mcr.bridge.mixin;

import java.net.InetSocketAddress;
import java.util.Optional;
import net.minecraft.client.multiplayer.resolver.ResolvedServerAddress;
import net.minecraft.client.multiplayer.resolver.ServerAddress;
import net.minecraft.client.multiplayer.resolver.ServerNameResolver;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Rust resolves names (SRV included); Java keeps the address unresolved and never queries DNS. */
@Mixin(ServerNameResolver.class)
abstract class ServerNameResolverMixin {
    @Inject(method = "resolveAddress", at = @At("HEAD"), cancellable = true)
    private void mcr$unresolved(ServerAddress address, CallbackInfoReturnable<Optional<ResolvedServerAddress>> cir) {
        InetSocketAddress unresolved = InetSocketAddress.createUnresolved(address.getHost(), address.getPort());
        cir.setReturnValue(Optional.of(new ResolvedServerAddress() {
            @Override
            public String getHostName() {
                return unresolved.getHostString();
            }

            @Override
            public String getHostIp() {
                return unresolved.getHostString();
            }

            @Override
            public int getPort() {
                return unresolved.getPort();
            }

            @Override
            public InetSocketAddress asInetSocketAddress() {
                return unresolved;
            }
        }));
    }
}
