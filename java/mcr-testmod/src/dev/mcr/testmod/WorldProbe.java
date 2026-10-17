package dev.mcr.testmod;

import dev.mcr.bridge.Bridge;
import dev.mcr.bridge.McrWorldScreen;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.StringJoiner;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.client.gui.screens.worldselection.WorldSelectionList;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.storage.LevelStorageSource;

/**
 * Test hooks for Java's world screens (`-Dmcr.testmod.worldProbe=<folder>`): reports the world list and selects its
 * first entry, opens the folder like a mod would while mcr's world is busy, and loads a world through
 * {@code doWorldLoad} when the file {@code mcr-probe-load} appears in the game dir.
 */
final class WorldProbe {
    private static final String FOLDER = System.getProperty("mcr.testmod.worldProbe", "");
    private static final Path LOAD_TRIGGER = Path.of("mcr-probe-load");

    private static Screen listed;
    private static Screen accessed;

    private WorldProbe() {}

    static void tick(Minecraft client) {
        if (FOLDER.isEmpty()) {
            return;
        }
        Screen screen = client.gui.screen();
        if (screen instanceof SelectWorldScreen && screen != listed) {
            listWorlds(screen);
        }
        if (screen instanceof McrWorldScreen && screen != accessed) {
            accessed = screen;
            Bridge.info("world_access", tryAccess(client));
        }
        if (Files.exists(LOAD_TRIGGER)) {
            try {
                Files.delete(LOAD_TRIGGER);
            } catch (java.io.IOException e) {
                return;
            }
            loadThroughVanilla(client);
        }
    }

    private static void listWorlds(Screen screen) {
        for (GuiEventListener child : screen.children()) {
            if (!(child instanceof WorldSelectionList list)) {
                continue;
            }
            StringJoiner names = new StringJoiner("|");
            WorldSelectionList.WorldListEntry first = null;
            for (WorldSelectionList.Entry entry : list.children()) {
                if (entry instanceof WorldSelectionList.WorldListEntry world) {
                    names.add(world.getLevelName());
                    if (first == null) {
                        first = world;
                    }
                }
            }
            if (first != null) {
                listed = screen;
                list.setSelected(first);
                Bridge.info("world_list", names.toString());
            }
        }
    }

    private static String tryAccess(Minecraft client) {
        try (LevelStorageSource.LevelStorageAccess access = client.getLevelSource().validateAndCreateAccess(FOLDER)) {
            return "opened " + access.getLevelId();
        } catch (Exception e) {
            return "refused: " + e.getMessage();
        }
    }

    private static void loadThroughVanilla(Minecraft client) {
        LevelSettings settings = new LevelSettings(
                "Probe Load", GameType.CREATIVE, LevelSettings.DifficultySettings.DEFAULT, true,
                WorldDataConfiguration.DEFAULT);
        client.createWorldOpenFlows().createFreshLevel(
                "mcr-probe-load", settings, new WorldOptions(1L, false, false), WorldPresets::createNormalWorldDimensions,
                new TitleScreen());
        boolean blocked = client.getSingleplayerServer() == null && client.level == null
                && client.gui.screen() instanceof TitleScreen;
        Bridge.info("world_load", blocked ? "blocked" : "loaded");
    }
}
