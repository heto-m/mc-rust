package dev.mcr.bridge;

import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.fabricmc.loader.api.entrypoint.EntrypointContainer;
import net.fabricmc.loader.api.entrypoint.PreLaunchEntrypoint;

/** Fabric's entry; NeoForge's is {@code dev.mcr.bridge.neoforge.McrBridgeNeoForge}. */
public final class McrBridge implements ClientModInitializer, PreLaunchEntrypoint {
    @Override
    public void onPreLaunch() {
        QuietRefusals.install();
    }

    @Override
    public void onInitializeClient() {
        FabricLoader loader = FabricLoader.getInstance();
        List<Loaders.Mod> mods = new ArrayList<>();
        for (ModContainer mod : loader.getAllMods()) {
            ModContainer top = mod;
            while (top.getContainingMod().isPresent()) {
                top = top.getContainingMod().get();
            }
            var meta = mod.getMetadata();
            mods.add(new Loaders.Mod(meta.getId(), meta.getVersion().getFriendlyString(), top.getMetadata().getId(),
                    mod.getRootPaths()));
        }
        Loaders.install("fabric", mods);
        String entrypoints = loader.getEntrypointContainers("client", ClientModInitializer.class).stream()
                .map(EntrypointContainer::getProvider)
                .map(mod -> mod.getMetadata().getId())
                .distinct()
                .sorted()
                .collect(Collectors.joining(","));
        BridgeInit.run(entrypoints, McrBridge.class);
    }
}
