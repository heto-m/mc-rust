package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.device.GpuSurface;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import com.mojang.renderpearl.backend.api.CommandEncoderBackend;
import com.mojang.renderpearl.backend.api.GpuSurfaceBackend;
import dev.mcr.bridge.BridgeAbi;
import dev.mcr.bridge.McrFrame;
import java.util.Collection;
import java.util.EnumSet;

/** An offscreen RGBA8 image standing in for the window; unused while mcr composes Java's main target. */
final class McrSurface implements GpuSurfaceBackend {
    private final McrDeviceBackend device;
    private final long handle;

    McrSurface(McrDeviceBackend device, long handle) {
        this.device = device;
        this.handle = handle;
    }

    @Override
    public void configure(GpuSurface.Configuration config) {
        GpuNative.require(
                GpuNative.device(BridgeAbi.D_SURFACE_CONFIGURE, handle, config.width(), config.height()) + 1, "surface size");
    }

    @Override
    public boolean isSuboptimal() {
        return false;
    }

    @Override
    public void acquireNextTexture() {}

    @Override
    public void blitFromTexture(CommandEncoderBackend commandEncoder, GpuTextureView textureView) {
        if (!McrFrame.COMPOSE) {
            ((McrCommandEncoder) commandEncoder).blit(handle, GpuNative.handleOf(textureView));
        }
    }

    @Override
    public void present() {
        if (!McrFrame.COMPOSE) {
            GpuNative.check(GpuNative.device(BridgeAbi.D_SURFACE_PRESENT, handle), "present");
        }
    }

    @Override
    public Collection<GpuSurface.PresentMode> supportedPresentModes() {
        return EnumSet.allOf(GpuSurface.PresentMode.class);
    }

    @Override
    public void close() {
        device.destroyLater(handle);
    }
}
