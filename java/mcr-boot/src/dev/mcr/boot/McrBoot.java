package dev.mcr.boot;

import dev.mcr.boot.selftest.RoundTrip;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Arrays;

public final class McrBoot {
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
        Method main = Class.forName(args[1]).getMethod("main", String[].class);
        try {
            main.invoke(null, (Object) Arrays.copyOfRange(args, 2, args.length));
        } catch (InvocationTargetException e) {
            throw e.getCause();
        }
    }

    private McrBoot() {}
}
