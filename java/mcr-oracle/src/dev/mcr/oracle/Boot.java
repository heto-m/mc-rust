package dev.mcr.oracle;

import java.lang.reflect.Field;
import java.nio.file.Path;
import java.util.List;
import java.util.Optional;
import net.minecraft.SharedConstants;
import net.minecraft.client.resources.IndexedAssetSource;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.VanillaPackResources;
import net.minecraft.server.packs.VanillaPackResourcesBuilder;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.server.packs.resources.ResourceManager;

/** Brings up just enough of vanilla for a harness: registries, and the client jar's default pack on demand. */
public final class Boot {
    private static boolean registries;

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
            registries = true;
        }
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
