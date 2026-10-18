package dev.mcr.bridge.neoforge;

import dev.mcr.bridge.BridgeInit;
import dev.mcr.bridge.Loaders;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.ModList;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforgespi.language.IModInfo;

/** NeoForge's entry: FML constructs it while loading mods, with the mod list complete. */
@Mod(value = "mcr_bridge", dist = Dist.CLIENT)
public final class McrBridgeNeoForge {
    public McrBridgeNeoForge() {
        List<Loaders.Mod> mods = new ArrayList<>();
        for (IModInfo info : ModList.get().getMods()) {
            mods.add(new Loaders.Mod(info.getModId(), info.getVersion().toString(), info.getModId(),
                    List.of(info.getOwningFile().getFile().getFilePath())));
        }
        Loaders.install("neoforge", mods);
        Loaders.clientCommands(clientCommands());
        String ids = mods.stream().map(Loaders.Mod::id).sorted().collect(Collectors.joining(","));
        BridgeInit.run(ids, McrBridgeNeoForge.class);
    }

    /** NeoForge itself is a mod jar, not on the bridge's compile class path. */
    private static Predicate<String> clientCommands() {
        MethodHandle run;
        try {
            Class<?> handler = Class.forName("net.neoforged.neoforge.client.ClientCommandHandler", true,
                    McrBridgeNeoForge.class.getClassLoader());
            run = MethodHandles.publicLookup().findStatic(handler, "runCommand",
                    MethodType.methodType(boolean.class, String.class));
        } catch (ReflectiveOperationException e) {
            System.err.println("[mcr] NeoForge client commands unavailable: " + e);
            return command -> false;
        }
        return command -> {
            try {
                return (boolean) run.invokeExact(command);
            } catch (RuntimeException | Error e) {
                throw e;
            } catch (Throwable e) {
                throw new IllegalStateException(e);
            }
        };
    }
}
