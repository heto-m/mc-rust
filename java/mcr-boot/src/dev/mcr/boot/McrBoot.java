package dev.mcr.boot;

import dev.mcr.boot.selftest.RoundTrip;

public final class McrBoot {
    public static void main(long table, String[] args) {
        Native.init(table);
        String mode = args.length > 0 ? args[0] : "";
        switch (mode) {
            case "selftest" -> RoundTrip.run();
            case "selftest-exit" -> RoundTrip.exit();
            case "bench" -> RoundTrip.bench();
            default -> System.out.println("mcr-boot: nothing to run for mode '" + mode + "'");
        }
    }

    private McrBoot() {}
}
