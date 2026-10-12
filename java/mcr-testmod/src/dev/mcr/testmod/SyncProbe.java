package dev.mcr.testmod;

import com.mojang.blaze3d.platform.InputConstants;
import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.McrParticles;
import java.io.IOException;
import java.net.ProxySelector;
import java.net.URI;
import java.net.URL;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.Particle;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.locale.Language;
import net.minecraft.world.phys.Vec3;

/**
 * Test hooks for what mcr syncs into Java (`-Dmcr.testmod.syncProbe=true`): reports watched key mappings, the
 * effective render distance, entity scale, a translated string and dropped particles when they change, and
 * rebinds a mapping the way Controlling does once a watched mapping reached a value.
 */
final class SyncProbe {
    static final boolean ENABLED = Boolean.getBoolean("mcr.testmod.syncProbe");
    /** `name,name`: mappings whose key is reported as `key_<name>`. */
    private static final String[] WATCH = System.getProperty("mcr.testmod.watchKeys", "").split(",");
    /** `name=key`: the mapping rebound once {@link #REBIND_WHEN} holds. */
    private static final String REBIND = System.getProperty("mcr.testmod.rebind", "");
    /** `name=key`: a watched mapping that must hold this key before the rebind. */
    private static final String REBIND_WHEN = System.getProperty("mcr.testmod.rebindWhen", "");
    /** A URL fetched once at init through Java's proxy, like a mod's update check. */
    private static final String WEB_CHECK = System.getProperty("mcr.testmod.webCheck", "");

    private static final Map<String, String> reported = new HashMap<>();
    private static boolean rebound;

    private SyncProbe() {}

    static void init() {
        if (WEB_CHECK.isEmpty()) {
            return;
        }
        Thread thread = new Thread(SyncProbe::webCheck, "mcr_testmod update check");
        thread.setDaemon(true);
        thread.start();
    }

    /** Both of Java's HTTP stacks, logged the way Mod Menu and IPN log a failed update check. */
    private static void webCheck() {
        try (HttpClient client = HttpClient.newBuilder()
                .proxy(ProxySelector.getDefault())
                .connectTimeout(Duration.ofSeconds(10))
                .build()) {
            client.send(HttpRequest.newBuilder(URI.create(WEB_CHECK)).build(), HttpResponse.BodyHandlers.discarding());
        } catch (IOException | InterruptedException e) {
            McrTestMod.LOGGER.error("Error checking for updates: ", e);
        }
        try {
            URL url = URI.create(WEB_CHECK).toURL();
            url.openConnection().getInputStream().close();
        } catch (IOException e) {
            McrTestMod.LOGGER.warn("Update check failed with message - {}", e.getMessage());
        }
        Bridge.info("web_check", "done");
    }

    static void tick(Minecraft client) {
        if (!ENABLED) {
            return;
        }
        for (String name : WATCH) {
            KeyMapping mapping = name.isEmpty() ? null : KeyMapping.get(name);
            if (mapping != null) {
                report("key_" + name, mapping.saveString());
            }
        }
        report("effective_rd", Integer.toString(client.options.getEffectiveRenderDistance()));
        report("entity_scale", Double.toString(client.options.entityDistanceScaling().get()));
        report("lang_multiplayer", Language.getInstance().getOrDefault("menu.multiplayer"));
        report("particles_dropped", Long.toString(McrParticles.dropped()));
        maybeRebind();
    }

    private static void maybeRebind() {
        if (rebound || REBIND.isEmpty() || REBIND_WHEN.isEmpty()) {
            return;
        }
        String[] when = REBIND_WHEN.split("=", 2);
        KeyMapping watched = KeyMapping.get(when[0]);
        if (watched == null || when.length < 2 || !watched.saveString().equals(when[1])) {
            return;
        }
        String[] to = REBIND.split("=", 2);
        KeyMapping mapping = KeyMapping.get(to[0]);
        if (mapping == null || to.length < 2) {
            return;
        }
        rebound = true;
        mapping.setKey(InputConstants.getKey(to[1]));
        KeyMapping.resetMapping();
        Minecraft.getInstance().options.save();
    }

    /** Mod code spawning a vanilla particle type in front of the player; mcr does not draw these. */
    static void spawnVanillaParticle(Minecraft client) {
        if (client.player == null) {
            return;
        }
        Vec3 at = client.player.getEyePosition().add(client.player.getLookAngle().scale(2.0));
        Particle particle = client.particleEngine.createParticle(ParticleTypes.HAPPY_VILLAGER, at.x, at.y, at.z, 0, 0, 0);
        Bridge.info("vanilla_particle_kept", particle != null);
    }

    private static void report(String key, String value) {
        if (!value.equals(reported.put(key, value))) {
            Bridge.info(key, value);
        }
    }
}
