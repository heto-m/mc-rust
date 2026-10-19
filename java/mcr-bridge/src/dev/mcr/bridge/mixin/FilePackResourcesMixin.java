package dev.mcr.bridge.mixin;

import dev.mcr.bridge.ZipIndex;
import java.util.Enumeration;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.server.packs.FilePackResources;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.PackType;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** Vanilla walks every entry of a pack zip for each directory it lists; big packs list hundreds of directories while loading. */
@Mixin(FilePackResources.class)
abstract class FilePackResourcesMixin {
    @Shadow
    @Final
    private String prefix;

    @Redirect(
            method = "listResources",
            at = @At(value = "INVOKE", target = "Ljava/util/zip/ZipFile;entries()Ljava/util/Enumeration;"))
    private Enumeration<? extends ZipEntry> mcr$indexedEntries(
            ZipFile zip, PackType type, String namespace, String directory, PackResources.ResourceOutput output) {
        String root = type.getDirectory() + "/" + namespace + "/" + directory + "/";
        return ZipIndex.startingWith(zip, prefix.isEmpty() ? root : prefix + "/" + root);
    }
}
