import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.IOException;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.packs.PackType;
import net.minecraft.network.chat.Component;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PathPackResources;
import net.minecraft.server.packs.repository.PackSource;
import java.util.Optional;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;

/** Dumps vanilla's creative tab contents with the data-pack loaded registries a real server uses. */
public final class CreativeExtractor {
    private CreativeExtractor() {}

    public static void main(String[] args) throws Exception {
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();

        var generated = new PathPackResources(
            new PackLocationInfo("generated", Component.literal("generated"), PackSource.BUILT_IN, Optional.empty()),
            Path.of(args[1]));
        var resources = new MultiPackResourceManager(
            PackType.SERVER_DATA, List.<net.minecraft.server.packs.PackResources>of(generated));
        LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
        var staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
        RegistryAccess.Frozen context = layers.getAccessForLoading(RegistryLayer.WORLD);
        List<HolderLookup.RegistryLookup<?>> worldContext = TagLoader.buildUpdatedLookups(context, staticTags);
        RegistryAccess.Frozen world =
            RegistryDataLoader.load(resources, worldContext, RegistryDataLoader.WORLD_REGISTRIES, Runnable::run).join();
        RegistryAccess.Frozen access = layers.replaceFrom(RegistryLayer.WORLD, world).compositeAccess();
        staticTags.forEach(t -> t.apply());
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(access).forEach(DataComponentInitializers.PendingComponents::apply);

        CreativeModeTabs.tryRebuildTabContents(FeatureFlags.REGISTRY.allFlags(), true, access);

        try (PrintWriter w = new PrintWriter(Files.newBufferedWriter(out.resolve("creative_tabs.txt"), StandardCharsets.UTF_8))) {
            for (CreativeModeTab tab : CreativeModeTabs.allTabs()) {
                String key = BuiltInRegistries.CREATIVE_MODE_TAB.getKey(tab).getPath();
                String title = ((TranslatableContents) tab.getDisplayName().getContents()).getKey();
                w.println("tab " + key + " " + tab.row() + " " + tab.column() + " " + (tab.isAlignedRight() ? 1 : 0)
                    + " " + tab.getType() + " " + (tab.showTitle() ? 1 : 0) + " " + (tab.canScroll() ? 1 : 0)
                    + " " + title + " " + hex(access, tab.getIconItem()));
                if (tab.getType() != CreativeModeTab.Type.CATEGORY) {
                    continue;
                }
                for (ItemStack stack : tab.getDisplayItems()) {
                    w.println("p " + features(stack) + " " + hex(access, stack));
                }
                for (ItemStack stack : tab.getSearchTabDisplayItems()) {
                    w.println("s " + features(stack) + " " + hex(access, stack));
                }
            }
        }
        System.exit(0);
    }

    private static String features(ItemStack stack) {
        Set<String> names = FeatureFlags.REGISTRY.toNames(stack.getItem().requiredFeatures()).stream()
            .map(Object::toString)
            .collect(Collectors.toCollection(java.util.TreeSet::new));
        return names.isEmpty() ? "-" : String.join(",", names);
    }

    private static String hex(RegistryAccess access, ItemStack stack) {
        ByteBuf raw = Unpooled.buffer();
        RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(raw, access);
        ItemStack.OPTIONAL_STREAM_CODEC.encode(buf, stack);
        StringBuilder sb = new StringBuilder();
        while (raw.isReadable()) {
            sb.append(String.format("%02x", raw.readUnsignedByte()));
        }
        return sb.toString();
    }
}
