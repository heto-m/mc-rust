package dev.mcr.golden;

import com.mojang.blaze3d.platform.NativeImage;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.TitleScreen;

/** `-Dmcr.golden.title=<png>`: after `-Dmcr.golden.title.ticks` ticks on the title screen, saves it and quits. */
public final class TitleShot implements ClientModInitializer {
    private static final String OUT = System.getProperty("mcr.golden.title");
    private static final int TICKS = Integer.getInteger("mcr.golden.title.ticks", 80);

    private int titleTicks;
    private boolean shot;

    @Override
    public void onInitializeClient() {
        if (OUT == null) {
            return;
        }
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            if (shot || client.gui.overlay() != null || !(client.gui.screen() instanceof TitleScreen)) {
                return;
            }
            if (++titleTicks >= TICKS) {
                shot = true;
                Screenshot.takeScreenshot(client.gameRenderer.mainRenderTarget(), image -> save(client, image));
            }
        });
    }

    private static void save(Minecraft client, NativeImage image) {
        try (image) {
            Path out = Path.of(OUT);
            Files.createDirectories(out.toAbsolutePath().getParent());
            Path tmp = out.resolveSibling(out.getFileName() + ".tmp");
            image.writeToFile(tmp);
            Files.move(tmp, out, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException e) {
            e.printStackTrace();
        }
        client.stop();
    }
}
