package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrLanScreen;
import dev.mcr.bridge.McrWorlds;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.FocusableTextWidget;
import net.minecraft.client.gui.layouts.GridLayout;
import net.minecraft.client.gui.layouts.LinearLayout;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.WorldOptionsScreen;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.network.chat.Component;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** World Options gets an Open to LAN section for mcr's local world, which has no integrated server. */
@Mixin(WorldOptionsScreen.class)
abstract class WorldOptionsScreenMixin extends Screen {
    protected WorldOptionsScreenMixin(Component title) {
        super(title);
    }

    @Shadow
    private FocusableTextWidget createCategoryHeader(Component title) {
        throw new AssertionError();
    }

    @Inject(method = "generalOptions", at = @At("TAIL"))
    private void mcr$lanOptions(LinearLayout content, @Nullable IntegratedServer server, CallbackInfo ci) {
        if (server != null || !McrWorlds.local()) {
            return;
        }
        GridLayout grid = content.addChild(new GridLayout());
        grid.defaultCellSetting().alignHorizontallyCenter();
        GridLayout.RowHelper helper = grid.columnSpacing(8).rowSpacing(4).createRowHelper(1);
        helper.addChild(createCategoryHeader(Component.translatable("options.worldOptions.multiplayer.title")));
        Screen self = this;
        Button lan = Button.builder(Component.translatable("menu.multiplayerOptions.button"), b -> minecraft.gui.setScreen(new McrLanScreen(self)))
                .width(308)
                .build();
        if (McrWorlds.published()) {
            lan.active = false;
            lan.setMessage(Component.translatable("commands.publish.alreadyPublished.lan", McrWorlds.publishedPort()));
        }
        helper.addChild(lan);
    }
}
