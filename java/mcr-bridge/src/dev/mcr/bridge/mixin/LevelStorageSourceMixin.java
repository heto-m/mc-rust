package dev.mcr.bridge.mixin;

import dev.mcr.bridge.McrWorlds;
import java.io.IOException;
import java.nio.file.Path;
import net.minecraft.world.level.storage.LevelStorageSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyVariable;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Java's world storage is mcr's saves folder, and stays shut while mcr's server has a world open. */
@Mixin(LevelStorageSource.class)
abstract class LevelStorageSourceMixin {
    @ModifyVariable(method = "<init>", at = @At("HEAD"), argsOnly = true, ordinal = 0)
    private static Path mcr$saves(Path baseDir) {
        return redirect(baseDir, "saves", System.getProperty("mcr.bridge.saves"));
    }

    @ModifyVariable(method = "<init>", at = @At("HEAD"), argsOnly = true, ordinal = 1)
    private static Path mcr$backups(Path backupDir) {
        return redirect(backupDir, "backups", System.getProperty("mcr.bridge.backups"));
    }

    private static Path redirect(Path dir, String name, String to) {
        if (to == null || to.isEmpty()) {
            return dir;
        }
        Path own = Path.of(System.getProperty("user.dir")).resolve(name).toAbsolutePath().normalize();
        return dir.toAbsolutePath().normalize().equals(own) ? Path.of(to) : dir;
    }

    @Inject(method = {"createAccess", "validateAndCreateAccess"}, at = @At("HEAD"))
    private void mcr$notWhileOpen(String levelId, CallbackInfoReturnable<?> cir) {
        if (McrWorlds.busy()) {
            throw Sneaky.<RuntimeException>raise(new IOException("the world is open in mcr"));
        }
    }

    private static final class Sneaky {
        @SuppressWarnings("unchecked")
        static <T extends Throwable> T raise(Throwable t) throws T {
            throw (T) t;
        }
    }
}
