package dev.mcr.oracle.item;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import dev.mcr.oracle.text.TextHarness;
import io.netty.buffer.Unpooled;
import java.lang.reflect.Field;
import java.util.HexFormat;
import java.util.List;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.RemotePlayer;
import net.minecraft.client.resources.language.ClientLanguage;
import net.minecraft.core.RegistryAccess;
import net.minecraft.locale.Language;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.AttributeMap;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

/** {@code ItemStack.getTooltipLines} over generated stacks, each line as draw runs; see mcr-parity's tooltip.rs. */
public final class TooltipHarness implements Harness {
    private final RegistryAccess.Frozen registries;
    private final Item.TooltipContext context;
    private final Player player;

    /** Only {@link #enabledFeatures} is ever called: the player's level answers what a vanilla server enables. */
    static final class FeatureLevel extends ClientLevel {
        FeatureLevel() {
            super(null, null, null, null, 0, 0, null, false, 0L, 0);
        }

        @Override
        public FeatureFlagSet enabledFeatures() {
            return FeatureFlags.VANILLA_SET;
        }
    }

    public TooltipHarness(Boot boot) {
        Language.inject(ClientLanguage.loadFrom(boot.clientResources(), List.of("en_us"), false));
        registries = Boot.registryAccess();
        context = Item.TooltipContext.of(registries);
        try {
            player = Boot.allocate(RemotePlayer.class);
            set(LivingEntity.class, "attributes", player, new AttributeMap(Player.createAttributes().build()));
            set(Entity.class, "level", player, Boot.allocate(FeatureLevel.class));
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("fake player", e);
        }
    }

    private static void set(Class<?> owner, String name, Object target, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(name);
        f.setAccessible(true);
        f.set(target, value);
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        ItemStack stack;
        try {
            byte[] bytes = HexFormat.of().parseHex(testCase.get("stack").getAsString());
            stack = ItemStack.OPTIONAL_STREAM_CODEC.decode(new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(bytes), registries));
        } catch (RuntimeException e) {
            System.err.println("[tooltip] stack: " + e);
            JsonObject error = new JsonObject();
            error.addProperty("error", "decode");
            return error;
        }
        TooltipFlag.Default flag = new TooltipFlag.Default(testCase.get("advanced").getAsBoolean(), testCase.get("creative").getAsBoolean());
        JsonArray lines = new JsonArray();
        for (Component line : stack.getTooltipLines(context, player, flag)) {
            lines.add(TextHarness.runs(line, true));
        }
        JsonObject out = new JsonObject();
        out.add("lines", lines);
        return out;
    }
}
