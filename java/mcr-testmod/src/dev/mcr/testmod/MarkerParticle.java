package dev.mcr.testmod;

import net.fabricmc.fabric.api.client.particle.v1.ParticleProviderRegistry;
import net.fabricmc.fabric.api.particle.v1.FabricParticleTypes;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.HeartParticle;
import net.minecraft.core.Registry;
import net.minecraft.core.particles.SimpleParticleType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.phys.Vec3;

/** A mod particle type, {@code mcr_testmod:marker}, spawned in front of the camera after each press of the test key. */
public final class MarkerParticle {
    static final SimpleParticleType TYPE = FabricParticleTypes.simple(true);
    private static final int BURST_TICKS = 100;
    private static final double DISTANCE = 2.5;

    private static int burst;

    private MarkerParticle() {}

    static void register() {
        Registry.register(BuiltInRegistries.PARTICLE_TYPE, Identifier.fromNamespaceAndPath("mcr_testmod", "marker"), TYPE);
        ParticleProviderRegistry.getInstance().register(TYPE, HeartParticle.Provider::new);
    }

    /** A burst runs: particles spawn and {@code CameraFovMixin} zooms. */
    public static boolean active() {
        return burst > 0;
    }

    static void start() {
        burst = BURST_TICKS;
    }

    static void tick(Minecraft client) {
        if (burst <= 0 || client.level == null || client.player == null) {
            return;
        }
        burst--;
        Vec3 at = client.player.getEyePosition().add(client.player.getLookAngle().scale(DISTANCE));
        for (int i = 0; i < 4; i++) {
            client.level.addParticle(TYPE, at.x + (i % 2) * 0.3 - 0.15, at.y - 0.6, at.z + (i / 2) * 0.3 - 0.15, 0, 0, 0);
        }
    }
}
