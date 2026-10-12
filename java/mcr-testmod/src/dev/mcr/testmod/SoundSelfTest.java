package dev.mcr.testmod;

import dev.mcr.bridge.Bridge;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.Musics;
import net.minecraft.sounds.SoundEvent;

/** Drives mcr's sound bridge test from the title screen when {@value #PROPERTY} is set. */
final class SoundSelfTest {
    static final String PROPERTY = "mcr.testmod.sound";
    static final int TONE_AT = 20;
    static final int TONE_DONE_AT = 60;
    static final int MUSIC_AT = 80;
    static final int MUSIC_DONE_AT = MUSIC_AT + 30 * 20;
    static final int STOP_AT = MUSIC_DONE_AT + 10;
    static final int STOPPED_AT = STOP_AT + 10;

    private static final SoundEvent TONE =
            SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("mcr_testmod", "tone"));

    private static int ticks = -1;

    static boolean enabled() {
        return Boolean.getBoolean(PROPERTY);
    }

    static void tick(Minecraft client) {
        if (ticks < 0 && !(client.gui.screen() instanceof TitleScreen)) {
            return;
        }
        ticks++;
        var sounds = client.getSoundManager();
        switch (ticks) {
            case TONE_AT -> {
                sounds.play(SimpleSoundInstance.forUI(TONE, 1.0F, 1.0F));
                Bridge.info("testmod_sound", "tone");
            }
            case TONE_DONE_AT -> Bridge.info("testmod_channels", sounds.getChannelDebugString());
            case MUSIC_AT -> {
                if (!client.getMusicManager().isPlayingMusic(Musics.MENU)) {
                    client.getMusicManager().startPlaying(Musics.MENU);
                }
                Bridge.info("testmod_sound", "music");
            }
            case MUSIC_DONE_AT -> {
                Bridge.info("testmod_music_playing", client.getMusicManager().isPlayingMusic(Musics.MENU));
                Bridge.info("testmod_channels", sounds.getChannelDebugString());
            }
            case STOP_AT -> {
                client.getMusicManager().stopPlaying();
                sounds.stop(null, null);
                Bridge.info("testmod_sound", "stopped");
            }
            case STOPPED_AT -> Bridge.info("testmod_channels", sounds.getChannelDebugString());
            default -> {}
        }
    }

    private SoundSelfTest() {}
}
