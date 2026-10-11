package dev.mcr.testmod;

import com.mojang.blaze3d.platform.NativeImage;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.world.item.Items;

/**
 * `-Dmcr.testmod.gui=<png>`: once the title screen is idle, show {@link GpuTestScreen} and save a screenshot.
 * `-Dmcr.testmod.gui.delay=<ticks>` stays on the title screen first; `-Dmcr.testmod.gui.settle=<ticks>` holds the test
 * screen before the screenshot (benchmarks use both).
 */
final class GuiCapture {
    static final String PROPERTY = "mcr.testmod.gui";
    private static final String QUIT_PROPERTY = "mcr.testmod.gui.quit";
    /** Item atlases and the blur settle within a few ticks; the default leaves ample margin. */
    private static final int SETTLE_TICKS = Integer.getInteger("mcr.testmod.gui.settle", 40);
    private static final int DELAY_TICKS = Integer.getInteger("mcr.testmod.gui.delay", 0);

    private static final String OUT = System.getProperty(PROPERTY);
    private static int ticks = -1;
    private static int titleTicks;

    static void tick(Minecraft minecraft) {
        if (OUT == null || minecraft.gui.overlay() != null) {
            return;
        }
        if (ticks < 0) {
            if (minecraft.gui.screen() instanceof TitleScreen && ++titleTicks > DELAY_TICKS) {
                bindItemComponents();
                minecraft.gui.setScreen(new GpuTestScreen());
                ticks = 0;
            }
            return;
        }
        if (++ticks != SETTLE_TICKS) {
            return;
        }
        Screenshot.takeScreenshot(minecraft.gameRenderer.mainRenderTarget(), image -> save(minecraft, image));
    }

    /** Item components are data-driven and normally bound on world join; icons on a menu need them earlier. */
    private static void bindItemComponents() {
        if (!BuiltInRegistries.ITEM.wrapAsHolder(Items.APPLE).areComponentsBound()) {
            BuiltInRegistries.DATA_COMPONENT_INITIALIZERS
                    .build(VanillaRegistries.createWorldLookup())
                    .forEach(DataComponentInitializers.PendingComponents::apply);
        }
    }

    private static void save(Minecraft minecraft, NativeImage image) {
        try (image) {
            Path out = Path.of(OUT);
            Files.createDirectories(out.toAbsolutePath().getParent());
            Path tmp = out.resolveSibling(out.getFileName() + ".tmp");
            image.writeToFile(tmp);
            Files.move(tmp, out, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            McrTestMod.LOGGER.info("gui capture written to {} ({}x{})", out, image.getWidth(), image.getHeight());
        } catch (IOException e) {
            McrTestMod.LOGGER.error("gui capture failed", e);
        }
        if (Boolean.getBoolean(QUIT_PROPERTY)) {
            minecraft.stop();
        }
    }

    private GuiCapture() {}
}
