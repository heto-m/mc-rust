package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Leashable;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.phys.AABB;

/**
 * The sections mcr's visibility graph reached from this frame's camera. Java's own graph sees through
 * every bridge section (they have no geometry), so block entities and entities in sections mcr ruled
 * out are dropped here.
 */
public final class McrSections {
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG_UNALIGNED;
    /** An entity spanning more sections than this per axis is never culled. */
    private static final int MAX_SPAN = 4;
    /** `-Dmcr.sections=false` keeps Java's own culling, for measurements. */
    private static final boolean ENABLED = !"false".equals(System.getProperty("mcr.sections"));

    private static MemorySegment bits = MemorySegment.NULL;
    private static boolean active;
    private static int ox;
    private static int oy;
    private static int oz;
    private static int dx;
    private static int dy;
    private static int dz;
    private static boolean blockEntities;
    private static long culledEntities;
    private static long culledSections;
    private static long lastCulledPos = Long.MAX_VALUE;
    private static BlockEntityType<?> lastCulledType;

    private McrSections() {}

    static void frame(MemorySegment shared, boolean present) {
        active = false;
        if (!present || !ENABLED) {
            return;
        }
        ox = shared.get(INT, BridgeAbi.SEC_ORIGIN);
        oy = shared.get(INT, BridgeAbi.SEC_ORIGIN + 4L);
        oz = shared.get(INT, BridgeAbi.SEC_ORIGIN + 8L);
        dx = shared.get(INT, BridgeAbi.SEC_DIMS);
        dy = shared.get(INT, BridgeAbi.SEC_DIMS + 4L);
        dz = shared.get(INT, BridgeAbi.SEC_DIMS + 8L);
        long volume = (long) dx * dy * dz;
        if (dx <= 0 || dy <= 0 || dz <= 0 || volume > BridgeAbi.SEC_MAX_BITS) {
            return;
        }
        bits = shared;
        active = McrFrame.camera() != null;
    }

    /** Whether mcr may see into the section; anything outside mcr's search box counts as seen. */
    public static boolean reached(int sx, int sy, int sz) {
        if (!active) {
            return true;
        }
        long x = (long) sx - ox;
        long y = (long) sy - oy;
        long z = (long) sz - oz;
        if (x < 0 || y < 0 || z < 0 || x >= dx || y >= dy || z >= dz) {
            return true;
        }
        long i = x + z * dx + y * dx * dz;
        long word = bits.get(LONG, BridgeAbi.SEC_BITS + (i >>> 6) * 8);
        return (word >>> (i & 63) & 1L) != 0;
    }

    /** Block entities of unreached sections are left out while vanilla extracts them. */
    public static void blockEntities(boolean on) {
        blockEntities = on && active;
    }

    public static boolean blockEntitySectionVisible(long sectionNode, BlockEntity first) {
        if (!blockEntities || reached(SectionPos.x(sectionNode), SectionPos.y(sectionNode), SectionPos.z(sectionNode))) {
            return true;
        }
        culledSections++;
        lastCulledPos = first.getBlockPos().asLong();
        lastCulledType = first.getType();
        return false;
    }

    /** Whether any section the entity's bounds touch was reached. */
    public static boolean entityVisible(Entity entity) {
        if (!active || entity instanceof Leashable leashable && leashable.isLeashed() || entity.isVehicle()) {
            return true;
        }
        AABB box = entity.getBoundingBox().inflate(0.5).expandTowards(0.0, 1.0, 0.0);
        int x0 = SectionPos.blockToSectionCoord(box.minX);
        int y0 = SectionPos.blockToSectionCoord(box.minY);
        int z0 = SectionPos.blockToSectionCoord(box.minZ);
        int x1 = SectionPos.blockToSectionCoord(box.maxX);
        int y1 = SectionPos.blockToSectionCoord(box.maxY);
        int z1 = SectionPos.blockToSectionCoord(box.maxZ);
        if (x1 - x0 >= MAX_SPAN || y1 - y0 >= MAX_SPAN || z1 - z0 >= MAX_SPAN) {
            return true;
        }
        for (int y = y0; y <= y1; y++) {
            for (int z = z0; z <= z1; z++) {
                for (int x = x0; x <= x1; x++) {
                    if (reached(x, y, z)) {
                        return true;
                    }
                }
            }
        }
        culledEntities++;
        return false;
    }

    public static long culledEntities() {
        return culledEntities;
    }

    public static long culledSections() {
        return culledSections;
    }

    /** The first block entity of the last culled section as "type@x,y,z", or "none". */
    public static String lastCulledSection() {
        long p = lastCulledPos;
        if (p == Long.MAX_VALUE || lastCulledType == null) {
            return "none";
        }
        return BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(lastCulledType) + "@"
                + BlockPos.getX(p) + "," + BlockPos.getY(p) + "," + BlockPos.getZ(p);
    }
}
