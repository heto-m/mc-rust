package dev.mcr.oracle;

import java.lang.reflect.Field;
import java.nio.file.Path;
import java.util.List;
import java.util.Optional;
import net.minecraft.SharedConstants;
import net.minecraft.client.ClientBootstrap;
import net.minecraft.client.resources.IndexedAssetSource;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.VanillaPackResources;
import net.minecraft.server.packs.VanillaPackResourcesBuilder;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.tags.BlockTags;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.level.block.Blocks;

/** Brings up just enough of vanilla for a harness: registries (common and client), and the client jar's default pack on demand. */
public final class Boot {
    private static boolean registries;
    private static boolean tags;
    private static RegistryAccess.Frozen access;

    private final Path assets;
    private final String assetIndex;
    private ResourceManager resources;

    public Boot(Path assets, String assetIndex) {
        this.assets = assets;
        this.assetIndex = assetIndex;
        registries();
    }

    public static synchronized void registries() {
        if (!registries) {
            SharedConstants.tryDetectVersion();
            Bootstrap.bootStrap();
            ClientBootstrap.bootstrap();
            registries = true;
        }
    }

    private static MultiPackResourceManager dataResources() {
        PackLocationInfo location =
                new PackLocationInfo("vanilla", Component.literal("vanilla"), PackSource.BUILT_IN, Optional.empty());
        VanillaPackResources pack = new VanillaPackResourcesBuilder().exposeNamespace("minecraft").pushJarResources().build(location);
        List<PackResources> layers = pack.asResourcesSupplier().openResources(location, null).toList();
        return new MultiPackResourceManager(PackType.SERVER_DATA, layers);
    }

    /** Binds the jar's data-pack tags to the built-in registries, for vanilla code that asks {@code state.is(tag)}. */
    public static synchronized void tags() {
        if (!tags) {
            try (MultiPackResourceManager data = dataResources()) {
                TagLoader.loadTagsForExistingRegistries(data, RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY))
                        .forEach(Registry.PendingTags::apply);
            }
            if (!Blocks.OAK_LEAVES.defaultBlockState().is(BlockTags.LEAVES)) {
                throw new IllegalStateException("block tags did not bind");
            }
            tags = true;
        }
    }

    /**
     * Built-in registries plus the jar's data-pack registries (enchantments, trims, banner patterns...), like a vanilla
     * server's; also binds the items' default components.
     */
    public static synchronized RegistryAccess.Frozen registryAccess() {
        if (access == null) {
            LayeredRegistryAccess<RegistryLayer> layers = RegistryLayer.createRegistryAccess();
            try (MultiPackResourceManager data = dataResources()) {
                List<Registry.PendingTags<?>> staticTags = TagLoader.loadTagsForExistingRegistries(data, layers.getLayer(RegistryLayer.STATIC));
                List<HolderLookup.RegistryLookup<?>> lookups =
                        TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
                RegistryAccess.Frozen world = RegistryDataLoader.load(data, lookups, RegistryDataLoader.WORLD_REGISTRIES, Runnable::run).join();
                if (!tags) {
                    staticTags.forEach(Registry.PendingTags::apply);
                    tags = true;
                }
                access = layers.replaceFrom(RegistryLayer.WORLD, world).compositeAccess();
                BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(access).forEach(DataComponentInitializers.PendingComponents::apply);
            }
        }
        return access;
    }

    /** The jar's default resource pack, layered with the launcher asset index like the real client. */
    public synchronized ResourceManager clientResources() {
        if (resources == null) {
            VanillaPackResourcesBuilder builder = new VanillaPackResourcesBuilder()
                    .exposeNamespace("minecraft", "realms")
                    .pushJarResources();
            if (assets != null) {
                builder.pushLayer().pushAssetPath(PackType.CLIENT_RESOURCES, IndexedAssetSource.createIndexFs(assets, assetIndex));
            }
            PackLocationInfo location =
                    new PackLocationInfo("vanilla", Component.literal("vanilla"), PackSource.BUILT_IN, Optional.empty());
            VanillaPackResources pack = builder.build(location);
            List<PackResources> layers = pack.asResourcesSupplier().openResources(location, null).toList();
            resources = new MultiPackResourceManager(PackType.CLIENT_RESOURCES, layers);
        }
        return resources;
    }

    /** An instance without running its constructor, for vanilla classes whose constructors need a GPU. */
    public static <T> T allocate(Class<T> type) throws ReflectiveOperationException {
        Class<?> unsafeType = Class.forName("sun.misc.Unsafe");
        Field field = unsafeType.getDeclaredField("theUnsafe");
        field.setAccessible(true);
        Object unsafe = field.get(null);
        return type.cast(unsafeType.getMethod("allocateInstance", Class.class).invoke(unsafe, type));
    }
}
