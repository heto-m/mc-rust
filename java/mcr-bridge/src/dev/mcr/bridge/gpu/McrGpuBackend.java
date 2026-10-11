package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.device.GpuBackend;
import com.mojang.renderpearl.api.device.GpuDebugOptions;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.frontend.FrontendGpuDevice;
import org.jspecify.annotations.Nullable;
import org.lwjgl.sdl.SDLVideo;

/** The only backend vanilla may pick: a fake SDL window plus a device that mcr records later. */
public final class McrGpuBackend implements GpuBackend {
    private static final long SDL_WINDOW_OPENGL = 0x2L;
    private static final long SDL_WINDOW_VULKAN = 0x10000000L;

    @Override
    public String getName() {
        return "mcr";
    }

    @Override
    public void loadLibrary() {}

    @Override
    public void unloadLibrary() {}

    @Override
    public long createWindow(@Nullable String title, int width, int height, long flags) {
        return SDLVideo.SDL_CreateWindow(title, width, height, flags & ~(SDL_WINDOW_OPENGL | SDL_WINDOW_VULKAN));
    }

    @Override
    public GpuDevice createDevice(GpuDebugOptions debugOptions) {
        return new FrontendGpuDevice(new NullDeviceBackend());
    }
}
