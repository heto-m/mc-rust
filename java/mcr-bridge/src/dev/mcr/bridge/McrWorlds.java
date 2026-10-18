package dev.mcr.bridge;

import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.client.gui.screens.AlertScreen;
import net.minecraft.client.gui.screens.ConfirmScreen;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.options.HasDifficultyReaction;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.client.gui.screens.worldselection.WorldCreationUiState;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentUtils;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.gamerules.GameRule;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.levelgen.FlatLevelSource;
import net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings;
import net.minecraft.world.level.levelgen.presets.WorldPreset;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Java's world screens over mcr's local worlds: Play and Create go to mcr, which runs the vanilla server and joins
 * it through the net bridge. Render thread only.
 */
public final class McrWorlds {
    private static final Logger LOGGER = LoggerFactory.getLogger("mcr-worlds");
    private static final int MAX_TEXT = 4096;
    private static final int MAX_INBOX = 64;
    private static final String EULA_URL = "https://aka.ms/MinecraftEULA";
    private static final SystemToast.SystemToastId TOAST = new SystemToast.SystemToastId();
    private static final List<String> FEATURE_PACKS = List.of("minecart_improvements", "redstone_experiments", "trade_rebalance");
    private static final long CRASH_SCREEN_HOLD_MS = 1000;

    enum State { IDLE, PREPARING, LOCAL, STOPPING }

    private record Message(int kind, String text) {}

    private static final ArrayDeque<Message> inbox = new ArrayDeque<>();
    private static State state = State.IDLE;
    private static int publishedPort = -1;
    private static Path pendingPackDir;
    private static DisconnectedScreen crashScreen;
    private static long crashUntil;
    private static boolean difficultyLocked;

    private McrWorlds() {}

    /** Java's world screens go to mcr only when mcr composes Java's GUI. */
    public static boolean enabled() {
        return McrInput.active();
    }

    /** Java must not open any world folder: mcr's server is starting, running or saving. */
    public static boolean busy() {
        return state != State.IDLE;
    }

    /** In (or joining) a world on mcr's local server. */
    public static boolean local() {
        return state == State.LOCAL;
    }

    /** mcr's local world is open to LAN. */
    public static boolean published() {
        return local() && publishedPort > 0;
    }

    public static int publishedPort() {
        return publishedPort;
    }

    /** Open to LAN's choices go to mcr, which answers with {@code WORLD_LAN}. */
    public static void openLan(int mode, boolean cheats, int port) {
        if (local() && publishedPort < 0) {
            Bridge.report(BridgeAbi.REPORT_WORLD, "lan\tmode=" + mode + "\tcheats=" + bit(cheats) + "\tport=" + port);
        }
    }

    /** World Options' difficulty for mcr's local world, which only mcr's server console can change for the owner. */
    public static void requestDifficulty(int id) {
        if (local()) {
            Bridge.report(BridgeAbi.REPORT_WORLD, "difficulty\t" + id);
        }
    }

    public static void requestDifficultyLock() {
        if (local()) {
            Bridge.report(BridgeAbi.REPORT_WORLD, "lockdifficulty");
        }
    }

    /** mcr locked the local world's difficulty; the server learns it only at its next start. */
    public static boolean difficultyLocked() {
        return difficultyLocked && local();
    }

    /** The next connection Rust opens is the local world's. */
    public static boolean joining() {
        return state == State.PREPARING || state == State.LOCAL;
    }

    public static void joined() {
        if (state == State.PREPARING) {
            state = State.LOCAL;
        }
    }

    public static Screen worldList() {
        return new SelectWorldScreen(new TitleScreen());
    }

    /** A toast instead of a new session while the last one has not stopped. */
    public static boolean refuseWhileBusy(Minecraft minecraft) {
        if (busy()) {
            toast(minecraft, "The last world is still saving");
            return true;
        }
        return false;
    }

    public static void play(Minecraft minecraft, String folder) {
        if (refuseWhileBusy(minecraft)) {
            return;
        }
        state = State.PREPARING;
        minecraft.gui.setScreen(McrWorldScreen.progress());
        Bridge.report(BridgeAbi.REPORT_WORLD, "play\t" + folder);
    }

    /**
     * Java's Create New World options as a {@code create} request, or a toast and null for what mcr cannot create.
     * {@code packDir} is Java's temporary pack folder, if any.
     */
    public static String createRequest(Minecraft minecraft, WorldCreationUiState ui, Path packDir) {
        String preset = presetName(ui.getWorldType().preset() == null ? Optional.empty() : ui.getWorldType().preset().unwrapKey());
        if (preset == null || ui.getGameMode() == WorldCreationUiState.SelectedGameMode.DEBUG) {
            toast(minecraft, "This world type is not supported by mcr yet");
            return null;
        }
        String mode = switch (ui.getGameMode()) {
            case HARDCORE -> "hardcore";
            case CREATIVE -> "creative";
            default -> "survival";
        };
        StringBuilder out = new StringBuilder("create\nname=").append(oneLine(ui.getName().trim()))
                .append("\nmode=").append(mode)
                .append("\ndifficulty=").append(ui.getDifficulty().getId())
                .append("\ncheats=").append(bit(ui.isAllowCommands()))
                .append("\npreset=").append(preset)
                .append("\nseed=").append(oneLine(ui.getSeed()))
                .append("\nstructures=").append(bit(ui.isGenerateStructures()))
                .append("\nbonus=").append(bit(ui.isBonusChest()))
                .append('\n');
        GameRules rules = ui.getGameRules();
        rules.availableRules().forEach(rule -> appendRule(out, rules, rule));
        List<String> experiments = new ArrayList<>();
        List<String> packs = new ArrayList<>();
        for (String id : ui.getSettings().dataConfiguration().dataPacks().getEnabled()) {
            if (id.startsWith("file/")) {
                packs.add(id);
            } else if (FEATURE_PACKS.contains(id)) {
                experiments.add(id);
            }
        }
        if (!experiments.isEmpty()) {
            out.append("experiments=").append(String.join(",", experiments)).append('\n');
        }
        if (!packs.isEmpty() && packDir != null) {
            out.append("packs=").append(oneLine(String.join(",", packs))).append('\n');
            out.append("packdir=").append(oneLine(packDir.toAbsolutePath().toString())).append('\n');
        }
        if (preset.equals("flat") && ui.getSettings().selectedDimensions().overworld() instanceof FlatLevelSource flat) {
            FlatLevelGeneratorSettings.CODEC
                    .encodeStart(ui.getSettings().worldgenLoadContext().createSerializationContext(JsonOps.INSTANCE), flat.settings())
                    .result()
                    .ifPresent(json -> out.append("flat=").append(oneLine(json.toString())).append('\n'));
        }
        return out.toString();
    }

    private static <T> void appendRule(StringBuilder out, GameRules rules, GameRule<T> rule) {
        T value = rules.get(rule);
        if (!value.equals(rule.defaultValue())) {
            out.append("rule.").append(rule.id()).append('=').append(oneLine(rule.serialize(value))).append('\n');
        }
    }

    /** Enabled packs mcr cannot load (mod packs), which a toast reports. */
    public static boolean dropsPacks(WorldCreationUiState ui) {
        return ui.getSettings().dataConfiguration().dataPacks().getEnabled().stream()
                .anyMatch(id -> !id.equals("vanilla") && !id.startsWith("file/") && !FEATURE_PACKS.contains(id));
    }

    /** A mod creating a world through Java's integrated server: refused before any folder exists. */
    public static void refuseFreshLevel(Minecraft minecraft, Screen parent) {
        toast(minecraft, "mcr creates worlds only from Create New World");
        minecraft.gui.setScreen(parent);
    }

    /** Starts creating; Java's temporary pack folder is kept until the session settles, then deleted. */
    public static void create(Minecraft minecraft, String request, Path packDir, boolean droppedPacks) {
        state = State.PREPARING;
        deletePendingPacks();
        pendingPackDir = packDir;
        minecraft.gui.setScreen(McrWorldScreen.progress());
        if (droppedPacks) {
            toast(minecraft, "Data packs from mods are not applied by mcr");
        }
        Bridge.report(BridgeAbi.REPORT_WORLD, request);
    }

    private static void deletePendingPacks() {
        Path dir = pendingPackDir;
        pendingPackDir = null;
        if (dir == null) {
            return;
        }
        try {
            Files.walkFileTree(dir, new SimpleFileVisitor<>() {
                @Override
                public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) throws IOException {
                    Files.deleteIfExists(file);
                    return FileVisitResult.CONTINUE;
                }

                @Override
                public FileVisitResult postVisitDirectory(Path d, IOException e) throws IOException {
                    Files.deleteIfExists(d);
                    return FileVisitResult.CONTINUE;
                }
            });
        } catch (IOException e) {
            LOGGER.warn("could not delete {}", dir, e);
        }
    }

    static void cancel(Minecraft minecraft) {
        state = State.STOPPING;
        Bridge.report(BridgeAbi.REPORT_WORLD, "cancel");
        minecraft.gui.setScreen(worldList());
    }

    /** Save and Quit left the world; Java waits for mcr's server to stop. */
    public static void leftWorld(Minecraft minecraft) {
        publishedPort = -1;
        difficultyLocked = false;
        if (crashScreen != null) {
            minecraft.gui.setScreen(crashScreen);
            return;
        }
        state = State.STOPPING;
        minecraft.gui.setScreen(McrWorldScreen.saving());
    }

    private static String presetName(Optional<ResourceKey<WorldPreset>> key) {
        if (key.isEmpty()) {
            return null;
        }
        ResourceKey<WorldPreset> k = key.get();
        if (k.equals(WorldPresets.NORMAL)) {
            return "normal";
        } else if (k.equals(WorldPresets.FLAT)) {
            return "flat";
        } else if (k.equals(WorldPresets.LARGE_BIOMES)) {
            return "large_biomes";
        } else if (k.equals(WorldPresets.AMPLIFIED)) {
            return "amplified";
        }
        return null;
    }

    private static String bit(boolean b) {
        return b ? "1" : "0";
    }

    private static String oneLine(String s) {
        return s.replace('\n', ' ').replace('\r', ' ');
    }

    public static void toast(Minecraft minecraft, String text) {
        SystemToast.addOrUpdate(minecraft.gui.toastManager(), TOAST, Component.literal("Singleplayer"), Component.literal(text));
    }

    /** {@code MSG_WORLD}, while the control ring drains. */
    static void receive(MemorySegment payload) {
        if (payload.byteSize() < 4 || inbox.size() >= MAX_INBOX) {
            return;
        }
        int kind = payload.get(ValueLayout.JAVA_INT_UNALIGNED, 0);
        int length = (int) Math.min(payload.byteSize() - 4, MAX_TEXT);
        String text = new String(payload.asSlice(4, length).toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
        inbox.add(new Message(kind, text));
    }

    /** Applies the messages received this frame, before the net bridge opens any connection. */
    static void pump(Minecraft minecraft) {
        if (crashScreen != null) {
            Screen current = minecraft.gui.screen();
            if (System.currentTimeMillis() > crashUntil) {
                crashScreen = null;
            } else if (current != crashScreen
                    && (current instanceof DisconnectedScreen || current instanceof TitleScreen || current instanceof McrWorldScreen)) {
                minecraft.gui.setScreen(crashScreen);
            }
        }
        Message m;
        while ((m = inbox.poll()) != null) {
            try {
                apply(minecraft, m);
            } catch (RuntimeException e) {
                LOGGER.error("world message {} failed", m.kind(), e);
            }
        }
    }

    private static void apply(Minecraft minecraft, Message m) {
        Screen screen = minecraft.gui.screen();
        switch (m.kind()) {
            case BridgeAbi.WORLD_STATUS -> {
                if (state == State.IDLE && minecraft.level == null) {
                    state = State.PREPARING;
                    screen = McrWorldScreen.progress();
                    minecraft.gui.setScreen(screen);
                }
                if (screen instanceof McrWorldScreen progress && !progress.isSaving()) {
                    progress.setStatus(text(m.text()));
                }
            }
            case BridgeAbi.WORLD_EULA -> {
                state = State.PREPARING;
                minecraft.gui.setScreen(new ConfirmScreen(
                        agreed -> answerEula(minecraft, agreed),
                        Component.literal("Minecraft EULA"),
                        Component.literal("Singleplayer runs Mojang's Minecraft server on this computer.\n"
                                + "By continuing you agree to the Minecraft EULA:\n" + EULA_URL),
                        Component.literal("I agree"),
                        Component.translatable("gui.cancel")));
            }
            case BridgeAbi.WORLD_READY -> {
                state = State.LOCAL;
                difficultyLocked = false;
                deletePendingPacks();
                if (screen instanceof McrWorldScreen progress && !progress.isSaving()) {
                    progress.setStatus(Component.translatable("connect.joining"));
                }
            }
            case BridgeAbi.WORLD_FAILED -> {
                state = State.IDLE;
                publishedPort = -1;
                deletePendingPacks();
                if (minecraft.level == null) {
                    fail(minecraft, m.text());
                }
            }
            case BridgeAbi.WORLD_STOPPED -> {
                boolean wasPlaying = state == State.LOCAL || state == State.STOPPING;
                state = State.IDLE;
                difficultyLocked = false;
                publishedPort = -1;
                deletePendingPacks();
                if (!m.text().isEmpty() && wasPlaying) {
                    crashed(minecraft, m.text());
                } else if (screen instanceof McrWorldScreen s && s.isSaving()) {
                    minecraft.gui.setScreen(new TitleScreen());
                } else if (screen instanceof McrWorldScreen) {
                    if (!m.text().isEmpty()) {
                        fail(minecraft, m.text());
                    } else {
                        minecraft.gui.setScreen(worldList());
                    }
                }
            }
            case BridgeAbi.WORLD_LAN -> lanAnswer(minecraft, m.text());
            case BridgeAbi.WORLD_LOCKED -> {
                difficultyLocked = true;
                if (screen instanceof HasDifficultyReaction reaction) {
                    reaction.onDifficultyChanged();
                }
            }
            default -> {}
        }
    }

    /** The local server stopped by itself: mcr's reason, and a way back to the world list. */
    private static void crashed(Minecraft minecraft, String reason) {
        crashScreen = new DisconnectedScreen(
                worldList(),
                Component.translatable("disconnect.lost"),
                Component.literal(reason),
                Component.translatable("gui.toWorld"));
        crashUntil = System.currentTimeMillis() + CRASH_SCREEN_HOLD_MS;
        minecraft.gui.setScreen(crashScreen);
    }

    private static void lanAnswer(Minecraft minecraft, String text) {
        Component message;
        if (text.startsWith("ok\t")) {
            String port = text.substring(3).trim();
            try {
                publishedPort = Integer.parseInt(port);
            } catch (NumberFormatException e) {
                return;
            }
            message = Component.translatable("menu.multiplayerOptions.publish.started.lan", ComponentUtils.copyOnClickText(port));
        } else {
            message = Component.translatable("commands.publish.failed");
        }
        if (minecraft.level != null) {
            minecraft.gui.hud.getChat().addClientSystemMessage(message);
        }
    }

    private static void answerEula(Minecraft minecraft, boolean agreed) {
        Bridge.report(BridgeAbi.REPORT_WORLD, agreed ? "eula\t1" : "eula\t0");
        if (agreed) {
            minecraft.gui.setScreen(McrWorldScreen.progress());
        } else {
            state = State.IDLE;
            minecraft.gui.setScreen(worldList());
        }
    }

    private static void fail(Minecraft minecraft, String reason) {
        minecraft.gui.setScreen(new AlertScreen(
                () -> minecraft.gui.setScreen(worldList()),
                Component.literal("The world could not be opened"),
                Component.literal(reason)));
    }

    /** {@code lang:key|arg|arg} as a translation, anything else as plain text. */
    static Component text(String status) {
        if (!status.startsWith("lang:")) {
            return Component.literal(status);
        }
        String[] parts = status.substring(5).split("\\|", 8);
        Object[] args = new Object[parts.length - 1];
        System.arraycopy(parts, 1, args, 0, args.length);
        return Component.translatable(parts[0], args);
    }
}
