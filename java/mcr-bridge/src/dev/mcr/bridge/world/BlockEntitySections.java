package dev.mcr.bridge.world;

import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher.RenderSection;

/**
 * The visible sections that hold block entities. Java's graph sees through every bridge section, so
 * its visible list is every section in the frustum; vanilla walks all of them each frame for block
 * entities. This keeps the few that have any until the list or a section's mesh changes.
 */
public final class BlockEntitySections {
    private static final ObjectArrayList<RenderSection> WITH_BLOCK_ENTITIES = new ObjectArrayList<>();
    private static volatile int version;
    private static int builtVersion = -1;
    private static ObjectArrayList<RenderSection> builtFrom;
    private static int builtSize = -1;

    private BlockEntitySections() {}

    /** Called when the visible list is cleared or any section's mesh is replaced. */
    public static void invalidate() {
        version++;
    }

    public static ObjectArrayList<RenderSection> of(ObjectArrayList<RenderSection> visible) {
        int current = version;
        if (current != builtVersion || visible != builtFrom || visible.size() != builtSize) {
            WITH_BLOCK_ENTITIES.clear();
            for (int i = 0, n = visible.size(); i < n; i++) {
                RenderSection section = visible.get(i);
                if (mayHaveBlockEntities(section.getSectionMesh())) {
                    WITH_BLOCK_ENTITIES.add(section);
                }
            }
            builtVersion = current;
            builtFrom = visible;
            builtSize = visible.size();
        }
        return WITH_BLOCK_ENTITIES;
    }

    private static boolean mayHaveBlockEntities(SectionMesh mesh) {
        return mesh instanceof BridgeSectionMesh bridge ? bridge.hasBlockEntities() : !mesh.getRenderableBlockEntities().isEmpty();
    }
}
