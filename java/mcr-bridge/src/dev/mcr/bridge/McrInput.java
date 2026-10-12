package dev.mcr.bridge;

import com.mojang.blaze3d.platform.Window;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.VarHandle;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.InputQuirks;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.client.input.PreeditEvent;
import net.minecraft.network.chat.Component;

/**
 * Rust's input in place of SDL's: events from the control ring go to the handlers vanilla's
 * {@code SDLEventHandler} calls, and Java's window wishes (grab, cursor, text input, quit) go back
 * through the shared input state. Only the render thread calls in here, except the clipboard.
 */
public final class McrInput {
    public static final String PROBE_PROPERTY = "mcr.bridge.probeInput";
    public static final int NATIVE_MENU = 0;

    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED;
    private static final VarHandle STATE_LONG = ValueLayout.JAVA_LONG.varHandle();
    private static final VarHandle STATE_INT = ValueLayout.JAVA_INT.varHandle();
    private static final int MAX_QUEUED = 4096;
    private static final int MAX_BUTTON = 32;
    private static final int SCANCODES = 512;
    private static final boolean PROBE = Boolean.getBoolean(PROBE_PROPERTY);

    private record Event(int type, int a, int b, int c, int d, double x, double y, double dx, double dy, String text) {}

    private static final ArrayDeque<Event> QUEUE = new ArrayDeque<>();
    private static boolean textInput;
    private static boolean quit;
    private static int cursor;
    private static int nativeSeq;
    private static String probed = "";

    /** Rust delivers input and serves the window only while it composes Java's frames. */
    public static boolean active() {
        return McrFrame.COMPOSE;
    }

    private static MemorySegment state() {
        return Bridge.get().inputState;
    }

    // ---- events (render thread) ----

    static void enqueue(MemorySegment p) {
        if (p.byteSize() < BridgeAbi.IN_TEXT_DATA || QUEUE.size() >= MAX_QUEUED) {
            return;
        }
        int type = p.get(INT, BridgeAbi.IN_TYPE);
        String text = null;
        if (type == BridgeAbi.IN_TEXT || type == BridgeAbi.IN_PREEDIT || type == BridgeAbi.IN_DROP) {
            int len = p.get(INT, BridgeAbi.IN_D);
            long room = p.byteSize() - BridgeAbi.IN_TEXT_DATA;
            if (len < 0 || len > room || len > BridgeAbi.INPUT_MAX_TEXT) {
                return;
            }
            byte[] bytes = p.asSlice(BridgeAbi.IN_TEXT_DATA, len).toArray(ValueLayout.JAVA_BYTE);
            text = new String(bytes, StandardCharsets.UTF_8);
        }
        QUEUE.add(new Event(
                type,
                p.get(INT, BridgeAbi.IN_A),
                p.get(INT, BridgeAbi.IN_B),
                p.get(INT, BridgeAbi.IN_C),
                p.get(INT, BridgeAbi.IN_D),
                finite(p.get(DOUBLE, BridgeAbi.IN_X)),
                finite(p.get(DOUBLE, BridgeAbi.IN_Y)),
                finite(p.get(DOUBLE, BridgeAbi.IN_DX)),
                finite(p.get(DOUBLE, BridgeAbi.IN_DY)),
                text));
    }

    private static double finite(double v) {
        return Double.isFinite(v) ? Math.clamp(v, -1.0e7, 1.0e7) : 0.0;
    }

    /** Runs the queued events in arrival order, as SDL's event poll would before the tick. */
    static void deliver(Minecraft minecraft) {
        long handle = minecraft.getWindow().handle();
        Event e;
        while ((e = QUEUE.poll()) != null) {
            switch (e.type) {
                case BridgeAbi.IN_KEY -> {
                    if (e.a > 0 && e.a < SCANCODES && e.d >= -1 && e.d <= 1) {
                        minecraft.keyboardHandler.keyPress(handle, e.d, new KeyEvent(e.a, e.b, e.c & 0xFFFF));
                    }
                }
                case BridgeAbi.IN_TEXT -> minecraft.keyboardHandler.textInput(handle, e.text);
                case BridgeAbi.IN_PREEDIT ->
                    minecraft.keyboardHandler.textEditing(handle, PreeditEvent.fromSdlTextEditing(e.text, e.a, e.b));
                case BridgeAbi.IN_MOTION -> minecraft.mouseHandler.onMove(handle, e.x, e.y, e.dx, e.dy);
                case BridgeAbi.IN_BUTTON -> {
                    if (e.a > 0 && e.a <= MAX_BUTTON) {
                        minecraft.mouseHandler.onButton(handle, new MouseButtonInfo(e.a, e.b & 0xFFFF), e.c != 0 ? 1 : 0);
                    }
                }
                case BridgeAbi.IN_WHEEL -> {
                    boolean inverted = InputQuirks.SHIFT_INVERTS_SCROLL_AXIS && e.dy == 0.0 && e.dx != 0.0 && (mods() & 3) != 0;
                    minecraft.mouseHandler.onScroll(handle, inverted ? 0.0 : e.dx, inverted ? -e.dx : e.dy);
                }
                case BridgeAbi.IN_DROP -> {
                    List<String> paths = new ArrayList<>();
                    for (String path : e.text.split("\0")) {
                        if (!path.isEmpty()) {
                            paths.add(path);
                        }
                    }
                    if (!paths.isEmpty()) {
                        minecraft.mouseHandler.onDrop(handle, List.copyOf(paths));
                    }
                }
                default -> {}
            }
        }
    }

    // ---- Rust's state ----

    public static boolean isKeyDown(int key) {
        if (key < 0 || key >= SCANCODES) {
            return false;
        }
        long word = (long) STATE_LONG.getAcquire(state(), (long) BridgeAbi.ISTATE_KEYS + (key >> 6) * 8L);
        return (word & (1L << (key & 63))) != 0;
    }

    public static int mods() {
        return (int) STATE_INT.getAcquire(state(), (long) BridgeAbi.ISTATE_MODS) & 0xFFFF;
    }

    public static boolean focused() {
        return ((int) STATE_INT.getAcquire(state(), (long) BridgeAbi.ISTATE_RUST_FLAGS) & BridgeAbi.RUST_FOCUSED) != 0;
    }

    // ---- Java's wishes ----

    /** Screen, overlay, grab and text input as of now; called every frame and on screen changes. */
    public static void publish(Minecraft minecraft) {
        int flags = 0;
        if (minecraft.gui.screen() != null) {
            flags |= BridgeAbi.JAVA_SCREEN;
        }
        if (minecraft.gui.overlay() != null) {
            flags |= BridgeAbi.JAVA_OVERLAY;
        }
        if (minecraft.mouseHandler != null && minecraft.mouseHandler.isMouseGrabbed()) {
            flags |= BridgeAbi.JAVA_GRABBED;
        }
        if (textInput) {
            flags |= BridgeAbi.JAVA_TEXT_INPUT;
        }
        if (quit) {
            flags |= BridgeAbi.JAVA_QUIT;
        }
        MemorySegment s = state();
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_JAVA_FLAGS, flags);
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_CURSOR, cursor);
        Window window = minecraft.getWindow();
        if (window != null) {
            STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_GUI_SCALE, window.getGuiScale());
        }
        if (PROBE) {
            probe(minecraft);
        }
    }

    public static void setTextInput(boolean on) {
        textInput = on;
    }

    public static void setTextArea(int x, int y, int width, int height) {
        MemorySegment s = state();
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_TEXT_AREA, x);
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_TEXT_AREA + 4, y);
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_TEXT_AREA + 8, width);
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_TEXT_AREA + 12, height);
    }

    /** Names from {@code CursorTypes}, in the order of mcr-modbridge's {@code CursorKind}. */
    public static void setCursor(String name) {
        cursor = switch (name) {
            case "arrow" -> 1;
            case "ibeam" -> 2;
            case "crosshair" -> 3;
            case "pointing_hand" -> 4;
            case "resize_ns" -> 5;
            case "resize_ew" -> 6;
            case "resize_all" -> 7;
            case "not_allowed" -> 8;
            default -> 0;
        };
    }

    public static void quit() {
        quit = true;
        Minecraft minecraft = Minecraft.getInstance();
        if (minecraft != null) {
            publish(minecraft);
        }
    }

    public static void openNative(int kind) {
        MemorySegment s = state();
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_NATIVE_KIND, kind);
        STATE_INT.setRelease(s, (long) BridgeAbi.ISTATE_NATIVE_SEQ, ++nativeSeq);
    }

    /** The "mcr" button on Java's title and pause screens: mcr's own settings and mod manager. */
    public static Button button() {
        return Button.builder(Component.literal("mcr"), b -> openNative(NATIVE_MENU))
                .bounds(4, 4, 40, 20)
                .tooltip(Tooltip.create(Component.literal("mcr settings and mods")))
                .build();
    }

    // ---- clipboard (any thread) ----

    public static String clipboard() {
        Bridge bridge = Bridge.get();
        int cap = 4096;
        try (Arena arena = Arena.ofConfined()) {
            for (int attempt = 0; attempt < 2; attempt++) {
                MemorySegment buf = arena.allocate(cap);
                int len = (int) bridge.clipboardGet.invokeExact(buf.address(), cap);
                if (len < 0) {
                    return "";
                }
                if (len <= cap) {
                    return new String(buf.asSlice(0, len).toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
                }
                cap = Math.min(len, BridgeAbi.INPUT_MAX_CLIPBOARD);
            }
            return "";
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    public static void setClipboard(String text) {
        byte[] bytes = text.getBytes(StandardCharsets.UTF_8);
        int len = Math.min(bytes.length, BridgeAbi.INPUT_MAX_CLIPBOARD);
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment buf = arena.allocate(Math.max(len, 1));
            MemorySegment.copy(bytes, 0, buf, ValueLayout.JAVA_BYTE, 0, len);
            Bridge.get().clipboardSet.invokeExact(buf.address(), len);
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
    }

    // ---- test probe ----

    /** Reports the focused text field's value when it changes (tests only). */
    private static void probe(Minecraft minecraft) {
        Screen screen = minecraft.gui.screen();
        String value = "";
        if (screen != null) {
            GuiEventListener focused = screen.getFocused();
            if (focused instanceof EditBox box) {
                value = box.getValue();
            }
        }
        if (!value.equals(probed)) {
            probed = value;
            Bridge.info("focus_value", value);
        }
    }

    /** Text fields of a new screen, `scale|x,y,w,h|...` in GUI units (tests only). */
    public static void probeEditBoxes(Screen screen) {
        if (!PROBE) {
            return;
        }
        StringBuilder out = new StringBuilder().append(Minecraft.getInstance().getWindow().getGuiScale());
        for (GuiEventListener child : screen.children()) {
            if (child instanceof EditBox box) {
                out.append('|').append(box.getX()).append(',').append(box.getY()).append(',')
                        .append(box.getWidth()).append(',').append(box.getHeight());
            }
        }
        Bridge.info("edit_boxes", out);
    }

    private McrInput() {}
}
