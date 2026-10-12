package dev.mcr.bridge;

import com.mojang.blaze3d.platform.InputConstants;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.server.DownloadedPackSource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Settings Java takes from mcr: vanilla key bindings both ways, the server packs mcr applied, and the render
 * distance. Render thread only.
 */
public final class McrSync {
    private static final Logger LOGGER = LoggerFactory.getLogger("mcr-sync");
    private static final int MAX_SYNC_BYTES = 16 << 10;
    private static final int MAX_NAME = 128;
    private static final int MAX_PATH = 1024;
    private static final int MIN_DISTANCE = 2;
    private static final int MAX_DISTANCE = 64;

    /** Synced vanilla mapping name to the key name Java last applied or reported. */
    private static final Map<String, String> keys = new LinkedHashMap<>();
    private static boolean keysSynced;
    private static boolean applying;
    /** Server pack ids Java loaded from mcr's files, by path. */
    private static final Map<String, UUID> packs = new LinkedHashMap<>();
    private static int renderDistance;
    private static double entityScale;

    private McrSync() {}

    // ---- key mappings ----

    /** {@code MSG_KEYS}: mcr's bindings replace Java's for every mapping named. */
    static void applyKeys(MemorySegment payload) {
        applying = true;
        try {
            for (String line : lines(payload)) {
                int tab = line.indexOf('\t');
                if (tab <= 0 || tab > MAX_NAME || line.length() - tab - 1 > MAX_NAME) {
                    continue;
                }
                String name = line.substring(0, tab);
                String key = line.substring(tab + 1);
                KeyMapping mapping = KeyMapping.get(name);
                if (mapping == null) {
                    continue;
                }
                try {
                    InputConstants.Key parsed = InputConstants.getKey(key);
                    if (!mapping.saveString().equals(key)) {
                        mapping.setKey(parsed);
                    }
                    keys.put(name, mapping.saveString());
                } catch (IllegalArgumentException e) {
                    LOGGER.warn("mcr sent an unknown key {} for {}", key, name);
                }
            }
            KeyMapping.resetMapping();
            keysSynced = true;
        } finally {
            applying = false;
        }
    }

    /** After a rebind or an options save: vanilla mappings that changed in Java go to mcr. */
    public static void keysMayHaveChanged() {
        if (!keysSynced || applying) {
            return;
        }
        StringBuilder out = new StringBuilder();
        for (Map.Entry<String, String> e : keys.entrySet()) {
            KeyMapping mapping = KeyMapping.get(e.getKey());
            if (mapping == null) {
                continue;
            }
            String now = mapping.saveString();
            if (!now.equals(e.getValue())) {
                e.setValue(now);
                out.append(e.getKey()).append('\t').append(now).append('\n');
            }
        }
        if (!out.isEmpty()) {
            Bridge.report(BridgeAbi.REPORT_KEYS, out.toString());
        }
    }

    // ---- server packs ----

    /** {@code MSG_PACKS}: the exact set of server pack files mcr applied, first pushed first. */
    static void applyPacks(Minecraft minecraft, MemorySegment payload) {
        List<String> wanted = new ArrayList<>();
        for (String line : lines(payload)) {
            if (line.length() <= MAX_PATH && Path.of(line).isAbsolute() && Files.isRegularFile(Path.of(line))) {
                wanted.add(line);
            } else {
                LOGGER.warn("mcr sent an unusable server pack path {}", line);
            }
        }
        DownloadedPackSource source = minecraft.getDownloadedPackSource();
        for (var it = packs.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<String, UUID> e = it.next();
            if (!wanted.contains(e.getKey())) {
                source.popPack(e.getValue());
                it.remove();
            }
        }
        boolean added = false;
        for (String path : wanted) {
            if (!packs.containsKey(path)) {
                UUID id = UUID.nameUUIDFromBytes(path.getBytes(StandardCharsets.UTF_8));
                if (!added) {
                    source.allowServerPacks();
                    added = true;
                }
                source.pushLocalPack(id, Path.of(path));
                packs.put(path, id);
            }
        }
        Bridge.info("server_packs", packs.size());
    }

    /** Java's pack manager dropped every server pack with its connection. */
    public static void packsCleared() {
        packs.clear();
    }

    // ---- render distance ----

    static void readView(MemorySegment frame) {
        int distance = frame.get(ValueLayout.JAVA_INT_UNALIGNED, BridgeAbi.FRAME_RENDER_DISTANCE);
        renderDistance = distance <= 0 ? 0 : Math.clamp(distance, MIN_DISTANCE, MAX_DISTANCE);
        float scale = frame.get(ValueLayout.JAVA_FLOAT_UNALIGNED, BridgeAbi.FRAME_ENTITY_SCALE);
        entityScale = Float.isFinite(scale) && scale > 0 ? (int) (Math.clamp(scale, 0.5, 5.0) * 4.0) / 4.0 : 0;
    }

    /** mcr's render distance while Java renders in its lockstep, 0 otherwise. */
    public static int renderDistance() {
        return McrFrame.lockstep() ? renderDistance : 0;
    }

    static void applyEntityScale(Minecraft minecraft) {
        if (entityScale > 0 && McrFrame.lockstep()) {
            var option = minecraft.options.entityDistanceScaling();
            if (option.get() != entityScale) {
                option.set(entityScale);
            }
        }
    }

    private static List<String> lines(MemorySegment payload) {
        int length = (int) Math.min(payload.byteSize(), MAX_SYNC_BYTES);
        String text = new String(payload.asSlice(0, length).toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
        List<String> out = new ArrayList<>();
        for (String line : text.split("\n")) {
            if (!line.isEmpty() && out.size() < BridgeAbi.SYNC_MAX_LINES) {
                out.add(line);
            }
        }
        return out;
    }
}
