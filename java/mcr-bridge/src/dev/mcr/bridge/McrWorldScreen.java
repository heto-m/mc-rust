package dev.mcr.bridge;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.network.chat.CommonComponents;
import net.minecraft.network.chat.Component;

/** mcr's local world progress (with Cancel) or the wait for its server to save and stop. */
public final class McrWorldScreen extends Screen {
    private static final long SAVING_TIMEOUT_MS = 120_000;

    private final boolean saving;
    private final long opened = System.currentTimeMillis();
    private Component status;

    private McrWorldScreen(Component title, boolean saving, Component status) {
        super(title);
        this.saving = saving;
        this.status = status;
    }

    public static McrWorldScreen progress() {
        return new McrWorldScreen(Component.translatable("createWorld.preparing"), false, Component.empty());
    }

    public static McrWorldScreen saving() {
        return new McrWorldScreen(Component.translatable("menu.savingLevel"), true, Component.empty());
    }

    public boolean isSaving() {
        return saving;
    }

    void setStatus(Component status) {
        this.status = status;
    }

    @Override
    protected void init() {
        if (!saving) {
            addRenderableWidget(Button.builder(CommonComponents.GUI_CANCEL, b -> McrWorlds.cancel(minecraft))
                    .bounds(width / 2 - 100, height / 4 + 120 + 12, 200, 20)
                    .build());
        }
    }

    @Override
    public void tick() {
        if (saving && System.currentTimeMillis() - opened > SAVING_TIMEOUT_MS) {
            minecraft.gui.setScreen(new TitleScreen());
        }
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        super.extractRenderState(graphics, mouseX, mouseY, a);
        graphics.centeredText(font, title, width / 2, height / 2 - 50, -1);
        graphics.centeredText(font, status, width / 2, height / 2 - 30, -1);
    }

    @Override
    public boolean shouldCloseOnEsc() {
        return false;
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }
}
