package dev.mcr.bridge;

import dev.mcr.boot.Native;
import java.util.stream.Collectors;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.fabricmc.loader.api.entrypoint.EntrypointContainer;

public final class McrBridge implements ClientModInitializer {
    public static final String TEST_PROPERTY = "mcr.bridge.test";

    @Override
    public void onInitializeClient() {
        FabricLoader loader = FabricLoader.getInstance();
        for (ModContainer mod : loader.getAllMods()) {
            var meta = mod.getMetadata();
            Bridge.report(BridgeAbi.REPORT_MOD_LOADED, meta.getId() + "\t" + meta.getVersion().getFriendlyString());
        }
        String entrypoints = loader.getEntrypointContainers("client", ClientModInitializer.class).stream()
                .map(EntrypointContainer::getProvider)
                .map(mod -> mod.getMetadata().getId())
                .distinct()
                .sorted()
                .collect(Collectors.joining(","));
        Bridge.info("client_entrypoints", entrypoints);
        Bridge.info("bridge_loader", McrBridge.class.getClassLoader().getClass().getName());
        Bridge.info("boot_loader", Native.class.getClassLoader().getClass().getName());
        switch (System.getProperty(TEST_PROPERTY, "")) {
            case "throw-in-init" -> throw new IllegalStateException("mcr_bridge test failure in client init");
            case "exit-in-init" -> System.exit(7);
            default -> {}
        }
        Bridge.report(BridgeAbi.REPORT_INIT, "mcr_bridge");
    }
}
