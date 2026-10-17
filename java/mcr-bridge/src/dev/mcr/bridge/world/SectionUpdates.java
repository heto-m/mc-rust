package dev.mcr.bridge.world;

import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher.RenderSection;

/**
 * Vanilla looks up every visible section's dirty state each frame to find the few to recompile; Java's
 * visible list is every section in the frustum, so that walk is skipped while no section turned dirty
 * and the list is the one already walked.
 */
public final class SectionUpdates {
    private static final ObjectArrayList<RenderSection> NONE = new ObjectArrayList<>();
    private static volatile int version;
    private static int walkedVersion = -1;
    private static Object walkedTracker;
    private static ObjectArrayList<RenderSection> walkedList;
    private static int walkedSize = -1;

    private SectionUpdates() {}

    /** Called when a section turns dirty or the visible list is cleared. */
    public static void changed() {
        version++;
    }

    public static ObjectArrayList<RenderSection> toWalk(Object tracker, ObjectArrayList<RenderSection> visible) {
        int current = version;
        if (current == walkedVersion && tracker == walkedTracker && visible == walkedList && visible.size() == walkedSize) {
            return NONE;
        }
        walkedVersion = current;
        walkedTracker = tracker;
        walkedList = visible;
        walkedSize = visible.size();
        return visible;
    }
}
