package dev.mcr.bridge;

import java.io.File;
import java.nio.file.Path;
import net.minecraft.CrashReport;
import net.minecraft.client.Minecraft;
import net.minecraft.util.MemoryReserve;
import org.jspecify.annotations.Nullable;

/** Vanilla would System.exit the whole mcr process; instead save the report, tell Rust and park. */
public final class McrCrash {
    public static void saveReportAndPark(@Nullable Minecraft minecraft, File gameDirectory, CrashReport report, int exitCode) {
        MemoryReserve.release();
        String path = "";
        try {
            Minecraft.saveReportAndShutdownSoundManager(minecraft, gameDirectory, report, exitCode);
            Path saved = report.getSaveFile();
            path = saved == null ? "" : saved.toAbsolutePath().toString();
        } catch (Throwable t) {
            t.printStackTrace();
        }
        Bridge.report(BridgeAbi.REPORT_CRASH, path);
        McrFrame.parkForever();
    }

    private McrCrash() {}
}
