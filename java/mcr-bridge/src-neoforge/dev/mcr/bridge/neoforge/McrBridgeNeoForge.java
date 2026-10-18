package dev.mcr.bridge.neoforge;

import dev.mcr.bridge.BridgeInit;
import dev.mcr.bridge.Loaders;
import java.util.ArrayList;
import java.util.List;
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
        String ids = mods.stream().map(Loaders.Mod::id).sorted().collect(Collectors.joining(","));
        BridgeInit.run(ids, McrBridgeNeoForge.class);
    }
}
