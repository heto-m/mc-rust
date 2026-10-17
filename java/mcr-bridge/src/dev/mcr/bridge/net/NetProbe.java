package dev.mcr.bridge.net;

import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.Arrays;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.chunk.status.ChunkStatus;

/** Answers Rust's world probes (tests and measurements) and keeps Java-side net timings. */
public final class NetProbe {
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG_UNALIGNED;
    private static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED;
    private static final int MAX_POSITIONS = 1024;
    private static final int MAX_RADIUS = 64;
    private static final int MAX_ENTITIES = 1 << 16;

    private static long chunkPackets;
    private static long chunkNanos;
    private static long chunkStart;

    private NetProbe() {}

    public static void chunkPacketStarted() {
        chunkStart = System.nanoTime();
    }

    public static void chunkPacketDone() {
        if (chunkStart != 0) {
            chunkNanos += System.nanoTime() - chunkStart;
            chunkPackets++;
            chunkStart = 0;
        }
    }

    static void answer(Minecraft minecraft, MemorySegment p, int session) {
        if (p.byteSize() < BridgeAbi.PROBE_POSITIONS) {
            return;
        }
        int request = p.get(INT, BridgeAbi.PROBE_REQUEST);
        int cx = p.get(INT, BridgeAbi.PROBE_CENTER_X);
        int cz = p.get(INT, BridgeAbi.PROBE_CENTER_Z);
        int radius = Math.clamp(p.get(INT, BridgeAbi.PROBE_RADIUS), 0, MAX_RADIUS);
        int flags = p.get(INT, BridgeAbi.PROBE_FLAGS);
        long room = (p.byteSize() - BridgeAbi.PROBE_POSITIONS) / 12;
        int count = (int) Math.min(Math.min(Integer.toUnsignedLong(p.get(INT, BridgeAbi.PROBE_COUNT)), room), MAX_POSITIONS);
        if ((flags & BridgeAbi.PROBE_GC) != 0) {
            System.gc();
        }
        ClientLevel level = minecraft.level;
        LocalPlayer player = minecraft.player;
        int[] chunks = new int[0];
        int chunkCount = 0;
        int[] blocks = new int[count];
        int[] entities = new int[0];
        int entityCount = 0;
        if (level != null) {
            int side = 2 * radius + 1;
            chunks = new int[2 * side * side];
            for (int dx = -radius; dx <= radius; dx++) {
                for (int dz = -radius; dz <= radius; dz++) {
                    if (level.getChunkSource().getChunk(cx + dx, cz + dz, ChunkStatus.FULL, false) != null) {
                        chunks[2 * chunkCount] = cx + dx;
                        chunks[2 * chunkCount + 1] = cz + dz;
                        chunkCount++;
                    }
                }
            }
            for (int i = 0; i < count; i++) {
                long at = BridgeAbi.PROBE_POSITIONS + 12L * i;
                BlockPos pos = new BlockPos(p.get(INT, at), p.get(INT, at + 4), p.get(INT, at + 8));
                blocks[i] = Block.getId(level.getBlockState(pos));
            }
            entities = new int[256];
            for (Entity entity : level.entitiesForRendering()) {
                if (entityCount == MAX_ENTITIES) {
                    break;
                }
                if (entityCount == entities.length) {
                    entities = Arrays.copyOf(entities, entities.length * 2);
                }
                entities[entityCount++] = entity.getId();
            }
        } else {
            Arrays.fill(blocks, -1);
        }
        int ints = 2 * chunkCount + count + entityCount;
        MemorySegment out = McrNet.reserve(BridgeAbi.ANSWER_DATA + 4L * ints);
        out.asSlice(0, BridgeAbi.ANSWER_DATA).fill((byte) 0);
        McrNet.header(out, 0, session);
        int answerFlags = level != null ? BridgeAbi.ANSWER_LEVEL : 0;
        if (player != null && PlayerShadow.appliedTick >= 0) {
            answerFlags |= BridgeAbi.ANSWER_PLAYER;
            out.set(LONG, BridgeAbi.ANSWER_PLAYER_TICK, PlayerShadow.appliedTick);
            for (int i = 0; i < 3; i++) {
                out.set(DOUBLE, BridgeAbi.ANSWER_POS + 8L * i, PlayerShadow.appliedPos[i]);
            }
        }
        Runtime rt = Runtime.getRuntime();
        out.set(INT, BridgeAbi.ANSWER_REQUEST, request);
        out.set(INT, BridgeAbi.ANSWER_FLAGS, answerFlags);
        out.set(LONG, BridgeAbi.ANSWER_HEAP, rt.totalMemory() - rt.freeMemory());
        out.set(LONG, BridgeAbi.ANSWER_CHUNK_PACKETS, chunkPackets);
        out.set(LONG, BridgeAbi.ANSWER_CHUNK_NANOS, chunkNanos);
        out.set(LONG, BridgeAbi.ANSWER_FRAMES, McrNet.framesFed);
        out.set(LONG, BridgeAbi.ANSWER_FEED_NANOS, McrNet.feedNanos);
        out.set(INT, BridgeAbi.ANSWER_HELD_COUNT, player == null ? -1 : player.getInventory().getSelectedItem().getCount());
        out.set(INT, BridgeAbi.ANSWER_SWINGS, PlayerShadow.swings);
        out.set(INT, BridgeAbi.ANSWER_TUTORIAL, minecraft.options.tutorialStep.ordinal());
        out.set(INT, BridgeAbi.ANSWER_CHUNKS, chunkCount);
        out.set(INT, BridgeAbi.ANSWER_BLOCKS, count);
        out.set(INT, BridgeAbi.ANSWER_ENTITIES, entityCount);
        long at = BridgeAbi.ANSWER_DATA;
        MemorySegment.copy(chunks, 0, out, INT, at, 2 * chunkCount);
        at += 8L * chunkCount;
        MemorySegment.copy(blocks, 0, out, INT, at, count);
        at += 4L * count;
        MemorySegment.copy(entities, 0, out, INT, at, entityCount);
        McrNet.pushSegment(BridgeAbi.OUT_PROBE, out.asSlice(0, BridgeAbi.ANSWER_DATA + 4L * ints));
        if ((flags & BridgeAbi.PROBE_DISCONNECT) != 0 && level != null) {
            minecraft.disconnectFromWorld(ClientLevel.DEFAULT_QUIT_MESSAGE);
        }
    }
}
