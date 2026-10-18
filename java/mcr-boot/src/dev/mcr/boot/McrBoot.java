package dev.mcr.boot;

import dev.mcr.boot.selftest.RoundTrip;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Arrays;

public final class McrBoot {
    /** Set where LWJGL is shared on the class path (NeoForge): SDL message boxes get no video driver to show on. */
    public static final String SDL_OFFSCREEN_PROPERTY = "mcr.boot.sdlOffscreen";

    public static void main(long table, String[] args) throws Throwable {
        Native.init(table);
        String mode = args.length > 0 ? args[0] : "";
        switch (mode) {
            case "selftest" -> RoundTrip.run();
            case "selftest-exit" -> RoundTrip.exit();
            case "bench" -> RoundTrip.bench();
            case "launch" -> launch(args);
            default -> System.out.println("mcr-boot: nothing to run for mode '" + mode + "'");
        }
    }

    /** {@code launch <main class> <args...>}: runs another program's {@code main(String[])} on this thread. */
    private static void launch(String[] args) throws Throwable {
        if (args.length < 2) {
            throw new IllegalArgumentException("launch needs a main class");
        }
        noDialogs();
        Method main = Class.forName(args[1]).getMethod("main", String[].class);
        try {
            main.invoke(null, (Object) Arrays.copyOfRange(args, 2, args.length));
        } catch (InvocationTargetException e) {
            throw e.getCause();
        }
    }

    /** FML reports fatal start-up errors in a Swing dialog, or an SDL message box when AWT is headless. */
    private static void noDialogs() throws ReflectiveOperationException {
        // Caches the -Djava.awt.headless=true answer before FML sets the property to false.
        java.awt.GraphicsEnvironment.isHeadless();
        if (Boolean.getBoolean(SDL_OFFSCREEN_PROPERTY)) {
            Class.forName("org.lwjgl.sdl.SDLHints")
                    .getMethod("SDL_SetHint", CharSequence.class, CharSequence.class)
                    .invoke(null, "SDL_VIDEO_DRIVER", "offscreen");
        }
    }

    private McrBoot() {}
}
