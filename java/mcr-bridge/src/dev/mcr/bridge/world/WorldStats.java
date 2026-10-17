package dev.mcr.bridge.world;

import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.McrSections;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.state.level.LevelRenderState;

/** Reports what Java's level render drew, about once a second, for the bridge tests and measurements. */
public final class WorldStats {
    private static final long PERIOD_NANOS = 1_000_000_000L;
    private static long next;
    private static long frames;

    private WorldStats() {}

    public static void frame(LevelRenderState state, boolean outline) {
        frames++;
        long now = System.nanoTime();
        if (now - next < 0) {
            return;
        }
        next = now + PERIOD_NANOS;
        Bridge.info("world_render", "frames=" + frames
                + " entities=" + state.entityRenderStates.size()
                + " block_entities=" + state.blockEntityRenderStates.size()
                + " outline_last_frame=" + outline
                + " scans=" + BridgeSectionMesh.scans()
                + " scan_us=" + BridgeSectionMesh.scanNanos() / 1000
                + " culled_entities=" + McrSections.culledEntities()
                + " culled_sections=" + McrSections.culledSections()
                + " culled_last=" + McrSections.lastCulledSection()
                + " particles=" + particles());
    }

    private static String particles() {
        String counts = Minecraft.getInstance().particleEngine.countParticles();
        return counts.substring(counts.lastIndexOf(' ') + 1);
    }
}
