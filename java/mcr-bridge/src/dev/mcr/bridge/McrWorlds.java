package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.toasts.SystemToast;
import net.minecraft.client.gui.screens.AlertScreen;
import net.minecraft.client.gui.screens.ConfirmScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.worldselection.SelectWorldScreen;
import net.minecraft.client.gui.screens.worldselection.WorldCreationUiState;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceKey;
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

    enum State { IDLE, PREPARING, LOCAL, STOPPING }

    private record Message(int kind, String text) {}

    private static final ArrayDeque<Message> inbox = new ArrayDeque<>();
    private static State state = State.IDLE;

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

    /** Java's Create New World options as a {@code create} request, or a toast and null for what mcr cannot create. */
    public static String createRequest(Minecraft minecraft, WorldCreationUiState ui) {
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
        return "create\nname=" + oneLine(ui.getName().trim())
                + "\nmode=" + mode
                + "\ndifficulty=" + ui.getDifficulty().getId()
                + "\ncheats=" + bit(ui.isAllowCommands())
                + "\npreset=" + preset
                + "\nseed=" + oneLine(ui.getSeed())
                + "\nstructures=" + bit(ui.isGenerateStructures())
                + "\nbonus=" + bit(ui.isBonusChest())
                + "\n";
    }

    public static void create(Minecraft minecraft, String request, boolean droppedPacks) {
        state = State.PREPARING;
        minecraft.gui.setScreen(McrWorldScreen.progress());
        if (droppedPacks) {
            toast(minecraft, "Data packs and experiments are not applied by mcr");
        }
        Bridge.report(BridgeAbi.REPORT_WORLD, request);
    }

    static void cancel(Minecraft minecraft) {
        state = State.STOPPING;
        Bridge.report(BridgeAbi.REPORT_WORLD, "cancel");
        minecraft.gui.setScreen(worldList());
    }

    /** Save and Quit left the world; Java waits for mcr's server to stop. */
    public static void leftWorld(Minecraft minecraft) {
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
                if (screen instanceof McrWorldScreen progress && !progress.isSaving()) {
                    progress.setStatus(Component.translatable("connect.joining"));
                }
            }
            case BridgeAbi.WORLD_FAILED -> {
                state = State.IDLE;
                if (minecraft.level == null) {
                    fail(minecraft, m.text());
                }
            }
            case BridgeAbi.WORLD_STOPPED -> {
                state = State.IDLE;
                if (screen instanceof McrWorldScreen s) {
                    if (s.isSaving()) {
                        minecraft.gui.setScreen(new TitleScreen());
                    } else if (!m.text().isEmpty()) {
                        fail(minecraft, m.text());
                    } else {
                        minecraft.gui.setScreen(worldList());
                    }
                }
            }
            default -> {}
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
