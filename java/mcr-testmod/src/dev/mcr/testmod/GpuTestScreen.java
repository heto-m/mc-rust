package dev.mcr.testmod;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/** A static screen for backend parity tests: text, buttons, item icons, a scissored list and a blurred backdrop. */
public final class GpuTestScreen extends Screen {
    private static final int LIST_LINES = 12;
    private static final int LIST_SCROLL = 7;

    public GpuTestScreen() {
        super(Component.literal("mcr gpu test"));
    }

    @Override
    protected void init() {
        int x = width / 2 - 100;
        addRenderableWidget(Button.builder(Component.literal("First button"), b -> {}).bounds(x, 40, 200, 20).build());
        addRenderableWidget(Button.builder(Component.literal("Second button"), b -> {}).bounds(x, 64, 98, 20).build());
        Button disabled = Button.builder(Component.literal("Disabled"), b -> {}).bounds(x + 102, 64, 98, 20).build();
        disabled.active = false;
        addRenderableWidget(disabled);
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        graphics.fillGradient(0, 0, width, height, 0xFF204080, 0xFF802040);
        for (int i = 0; i < 8; i++) {
            int x = i * width / 8;
            graphics.fill(x, height / 3, x + width / 16, height * 2 / 3, 0xFF000000 | (0x30A0E0 * (i + 1) & 0xFFFFFF));
        }
        graphics.centeredText(font, "BEHIND THE BLUR", width / 2, height / 2, 0xFFFFFFFF);
        graphics.nextStratum();
        graphics.blurBeforeThisStratum();
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        super.extractRenderState(graphics, -1, -1, a);
        graphics.centeredText(font, title, width / 2, 20, 0xFFFFFFFF);
        graphics.text(font, "Shadowed text 0123456789", 10, 100, 0xFFFFFF55, true);
        graphics.text(font, "Plain text, no shadow", 10, 112, 0xFF55FFFF, false);
        ItemStack[] items = {
            new ItemStack(Items.DIAMOND_SWORD), new ItemStack(Items.GRASS_BLOCK, 64), new ItemStack(Items.APPLE, 3),
            new ItemStack(Items.OAK_LOG)
        };
        for (int i = 0; i < items.length; i++) {
            graphics.item(items[i], 10 + i * 20, 130);
            graphics.itemDecorations(font, items[i], 10 + i * 20, 130);
        }
        int x0 = width / 2 + 20;
        int y0 = 100;
        int x1 = x0 + 120;
        int y1 = y0 + 50;
        graphics.fill(x0 - 1, y0 - 1, x1 + 1, y1 + 1, 0xFFA0A0A0);
        graphics.fill(x0, y0, x1, y1, 0xFF101010);
        graphics.enableScissor(x0, y0, x1, y1);
        for (int i = 0; i < LIST_LINES; i++) {
            graphics.text(font, "List entry " + i, x0 + 4, y0 + 2 + i * 10 - LIST_SCROLL, 0xFFE0E0E0);
        }
        graphics.disableScissor();
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }
}
