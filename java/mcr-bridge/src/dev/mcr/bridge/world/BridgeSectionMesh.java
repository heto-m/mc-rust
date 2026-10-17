package dev.mcr.bridge.world;

import java.util.ArrayList;
import java.util.List;
import dev.mcr.bridge.McrSections;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderDispatcher;
import net.minecraft.client.renderer.chunk.SectionMesh;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;

/**
 * A section "compiled" without geometry: Rust draws terrain, so Java only needs the section's
 * renderable block entities for vanilla's extraction, and every face sees every other (no cave
 * culling, mcr's depth hides what is behind terrain).
 */
public final class BridgeSectionMesh implements SectionMesh {
    public static final BridgeSectionMesh EMPTY = new BridgeSectionMesh(0L, List.of());

    private static long scans;
    private static long scanNanos;

    private final long sectionNode;
    private final List<BlockEntity> blockEntities;

    private BridgeSectionMesh(long sectionNode, List<BlockEntity> blockEntities) {
        this.sectionNode = sectionNode;
        this.blockEntities = blockEntities;
    }

    @Override
    public boolean facesCanSeeEachother(Direction from, Direction to) {
        return true;
    }

    @Override
    public List<BlockEntity> getRenderableBlockEntities() {
        if (blockEntities.isEmpty() || McrSections.blockEntitySectionVisible(sectionNode, blockEntities.get(0))) {
            return blockEntities;
        }
        return List.of();
    }

    /** Whether the section holds any renderable block entity, before mcr's per-frame culling. */
    public boolean hasBlockEntities() {
        return !blockEntities.isEmpty();
    }

    /** The section's block entities that have a renderer; one walk of the chunk's block entity map. */
    public static BridgeSectionMesh scan(long sectionNode) {
        long start = System.nanoTime();
        BridgeSectionMesh mesh = EMPTY;
        Minecraft minecraft = Minecraft.getInstance();
        ClientLevel level = minecraft.level;
        LevelChunk chunk = level == null
                ? null
                : level.getChunkSource().getChunk(SectionPos.x(sectionNode), SectionPos.z(sectionNode), ChunkStatus.FULL, false);
        if (chunk != null && !chunk.getBlockEntities().isEmpty()) {
            int minY = SectionPos.sectionToBlockCoord(SectionPos.y(sectionNode));
            BlockEntityRenderDispatcher renderers = minecraft.levelRenderer.blockEntityRenderDispatcher();
            List<BlockEntity> found = null;
            for (BlockEntity blockEntity : chunk.getBlockEntities().values()) {
                int y = blockEntity.getBlockPos().getY();
                if (y >= minY && y < minY + 16 && renderers.getRenderer(blockEntity) != null) {
                    if (found == null) {
                        found = new ArrayList<>();
                    }
                    found.add(blockEntity);
                }
            }
            if (found != null) {
                mesh = new BridgeSectionMesh(sectionNode, List.copyOf(found));
            }
        }
        scans++;
        scanNanos += System.nanoTime() - start;
        return mesh;
    }

    /** Scans so far and their total render-thread time, for the measurements. */
    public static long scans() {
        return scans;
    }

    public static long scanNanos() {
        return scanNanos;
    }
}
