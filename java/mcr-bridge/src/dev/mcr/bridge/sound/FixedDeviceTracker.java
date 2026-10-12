package dev.mcr.bridge.sound;

import com.mojang.blaze3d.audio.DeviceList;
import com.mojang.blaze3d.audio.DeviceTracker;
import java.util.List;

/** mcr-audio is the only device and it never changes, so {@code SoundEngine} never reloads for one. */
public final class FixedDeviceTracker implements DeviceTracker {
    public static final String DEVICE = "mcr";
    private static final DeviceList DEVICES = new DeviceList(DEVICE, List.of(DEVICE));

    @Override
    public DeviceList currentDevices() {
        return DEVICES;
    }

    @Override
    public void tick() {}

    @Override
    public void forceRefresh() {}
}
