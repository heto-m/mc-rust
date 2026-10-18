package dev.mcr.bridge;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.CycleButton;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.CommonComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.server.permissions.Permissions;
import net.minecraft.util.HttpUtil;
import net.minecraft.world.level.GameType;

/** Open to LAN for mcr's local world: game mode, cheats and port go to mcr, which publishes it. */
public final class McrLanScreen extends Screen {
    private static final Component PORT = Component.translatable("lanServer.port");
    private static final Component INVALID_PORT = Component.translatable("lanServer.port.invalid");
    private static final Component UNAVAILABLE_PORT = Component.translatable("lanServer.port.unavailable");
    private static final int VALID_COLOR = -2039584;
    private static final int INVALID_COLOR = -2142128;

    private final Screen lastScreen;
    private GameType mode = GameType.SURVIVAL;
    private boolean cheats;
    private boolean hardcore;
    private int port;
    private EditBox portEdit;
    private Button start;

    public McrLanScreen(Screen lastScreen) {
        super(Component.translatable("lanServer.title"));
        this.lastScreen = lastScreen;
    }

    @Override
    protected void init() {
        if (portEdit == null) {
            hardcore = minecraft.level != null && minecraft.level.getLevelData().isHardcore();
            if (minecraft.gameMode != null) {
                mode = minecraft.gameMode.getPlayerMode();
            }
            cheats = minecraft.player != null && minecraft.player.permissions().hasPermission(Permissions.COMMANDS_GAMEMASTER);
            if (hardcore) {
                mode = GameType.SURVIVAL;
                cheats = false;
            }
        }
        CycleButton<GameType> modeButton = addRenderableWidget(CycleButton.builder(GameType::getShortDisplayName, mode)
                .withValues(GameType.SURVIVAL, GameType.SPECTATOR, GameType.CREATIVE, GameType.ADVENTURE)
                .create(width / 2 - 155, 100, 150, 20, Component.translatable("selectWorld.gameMode"), (b, value) -> mode = value));
        CycleButton<Boolean> cheatsButton = addRenderableWidget(CycleButton.onOffBuilder(cheats)
                .create(width / 2 + 5, 100, 150, 20, Component.translatable("selectWorld.allowCommands"), (b, value) -> cheats = value));
        modeButton.active = !hardcore;
        cheatsButton.active = !hardcore;
        String typed = portEdit == null ? "" : portEdit.getValue();
        portEdit = new EditBox(font, width / 2 - 75, 160, 150, 20, PORT);
        portEdit.setMaxLength(5);
        portEdit.setValue(typed);
        portEdit.setResponder(this::checkPort);
        addRenderableWidget(portEdit);
        start = addRenderableWidget(Button.builder(Component.translatable("lanServer.start"), b -> {
                    McrWorlds.openLan(mode.getId(), cheats, port);
                    minecraft.gui.setScreen(null);
                })
                .bounds(width / 2 - 155, height - 28, 150, 20)
                .build());
        addRenderableWidget(Button.builder(CommonComponents.GUI_CANCEL, b -> onClose())
                .bounds(width / 2 + 5, height - 28, 150, 20)
                .build());
        checkPort(typed);
    }

    private void checkPort(String value) {
        Component error = null;
        port = 0;
        if (!value.isBlank()) {
            try {
                int parsed = Integer.parseInt(value.trim());
                if (parsed < 1024 || parsed > 65535) {
                    error = INVALID_PORT;
                } else if (!HttpUtil.isPortAvailable(parsed)) {
                    error = UNAVAILABLE_PORT;
                } else {
                    port = parsed;
                }
            } catch (NumberFormatException e) {
                error = INVALID_PORT;
            }
        }
        portEdit.setTextColor(error == null ? VALID_COLOR : INVALID_COLOR);
        portEdit.setTooltip(Tooltip.create(error == null ? PORT : error));
        if (start != null) {
            start.active = error == null;
        }
    }

    @Override
    public void onClose() {
        minecraft.gui.setScreen(lastScreen);
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a) {
        super.extractRenderState(graphics, mouseX, mouseY, a);
        graphics.centeredText(font, title, width / 2, 50, -1);
        graphics.centeredText(font, Component.translatable("lanServer.otherPlayers"), width / 2, 82, -1);
        graphics.centeredText(font, PORT, width / 2, 148, -1);
    }
}
