package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrFrame;
import dev.mcr.bridge.McrParticles;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** mcr renders the vanilla particles of the paths it mirrors; Java keeps mod types and mod-spawned ones. */
@Mixin(ParticleEngine.class)
abstract class ParticleEngineMixin {
    @Inject(method = "createParticle", at = @At("HEAD"), cancellable = true)
    private void mcr$rustDrawsVanillaParticles(
            ParticleOptions options, double x, double y, double z, double xa, double ya, double za,
            CallbackInfoReturnable<Particle> cir) {
        if (!McrFrame.COMPOSE) {
            return;
        }
        Identifier id = BuiltInRegistries.PARTICLE_TYPE.getKey(options.getType());
        if (id == null || Identifier.DEFAULT_NAMESPACE.equals(id.getNamespace()) && McrParticles.mcrDraws()) {
            McrParticles.countDropped();
            cir.setReturnValue(null);
        }
    }

    @Inject(method = "tick", at = @At("HEAD"))
    private void mcr$tickStart(CallbackInfo ci) {
        McrParticles.enter();
    }

    @Inject(method = "tick", at = @At("RETURN"))
    private void mcr$tickEnd(CallbackInfo ci) {
        McrParticles.exit();
    }
}
