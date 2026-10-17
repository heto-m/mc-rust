package dev.mcr.bridge;

import com.mojang.blaze3d.platform.SDLEventHandler;
import dev.mcr.bridge.mixin.SDLEventHandlerInvoker;
import org.lwjgl.sdl.SDL_Event;

/**
 * Shows Rust's key and button events to mods hooked into {@code SDLEventHandler} (malilib's hotkeys): each
 * one passes through vanilla's handler as an SDL event whose vanilla body is skipped, and a mod that cancels
 * it consumes the event.
 */
public final class McrRawInput {
    private static final int KEY_DOWN = 768;
    private static final int KEY_UP = 769;
    private static final int BUTTON_DOWN = 1025;
    private static final int BUTTON_UP = 1026;

    private static SDLEventHandler handler;
    private static SDL_Event event;
    private static boolean dispatching;
    private static boolean reached;
    private static boolean failed;

    private McrRawInput() {}

    /** {@code SDLEventHandler} constructor TAIL. */
    public static void capture(SDLEventHandler h) {
        handler = h;
    }

    /** Whether the key event is still vanilla's to handle. */
    static boolean key(int scancode, int key, int mods, int action) {
        SDL_Event ev = event();
        if (ev == null) {
            return true;
        }
        ev.key().type(action == 0 ? KEY_UP : KEY_DOWN).scancode(scancode).key(key).mod((short) mods)
                .down(action != 0).repeat(action == -1);
        return dispatch(ev, true);
    }

    /** Whether the button event is still vanilla's to handle. */
    static boolean button(int button, boolean pressed, double x, double y) {
        SDL_Event ev = event();
        if (ev == null) {
            return true;
        }
        ev.button().type(pressed ? BUTTON_DOWN : BUTTON_UP).button((byte) button).down(pressed).clicks((byte) 1)
                .x((float) x).y((float) y);
        return dispatch(ev, false);
    }

    private static SDL_Event event() {
        if (handler == null || failed) {
            return null;
        }
        if (event == null) {
            event = SDL_Event.create();
        }
        return event;
    }

    private static boolean dispatch(SDL_Event ev, boolean key) {
        dispatching = true;
        reached = false;
        try {
            if (key) {
                ((SDLEventHandlerInvoker) handler).mcr$handleKeyEvent(ev);
            } else {
                ((SDLEventHandlerInvoker) handler).mcr$handleMouseButtonEvent(ev);
            }
        } catch (Throwable t) {
            failed = true;
            reached = true;
            System.err.println("[mcr] SDL event hooks failed, mods stop seeing raw input: " + t);
        } finally {
            dispatching = false;
        }
        return reached;
    }

    /** {@code SDLEventHandler} handler HEAD, after every mod's: true skips vanilla's body. */
    public static boolean reachedVanilla() {
        if (dispatching) {
            reached = true;
        }
        return dispatching;
    }
}
