package dev.mcr.bridge.sound;

/** Mirror of mcr-modbridge's sound ring, control block and voice states; Rust tests compare both. */
public final class SoundAbi {
    public static final int SND_SLOT = 0;
    public static final int SND_ARG = 4;
    public static final int SND_X = 8;
    public static final int SND_Y = 12;
    public static final int SND_Z = 16;
    public static final int SND_SIZE = 20;

    public static final int SND_CREATE = 1;
    public static final int SND_DESTROY = 2;
    public static final int SND_PLAY = 3;
    public static final int SND_PAUSE = 4;
    public static final int SND_RESUME = 5;
    public static final int SND_STOP = 6;
    public static final int SND_POSITION = 7;
    public static final int SND_PITCH = 8;
    public static final int SND_VOLUME = 9;
    public static final int SND_LOOPING = 10;
    public static final int SND_ATTENUATION = 11;
    public static final int SND_RELATIVE = 12;
    public static final int SND_BUFFER = 13;
    public static final int SND_STREAM = 14;

    public static final int CTL_ATTACHED = 0;
    public static final int CTL_VOLUME_SEQ = 4;
    public static final int CTL_VOLUMES = 8;
    public static final int CTL_DROPPED = 52;
    public static final int CTL_SIZE = 64;

    public static final int SOURCES = 11;
    public static final int SOUND_SLOTS = 256;
    public static final int STREAM_END = -1;

    public static final int FORMAT_BITS = 0xFF;
    public static final int FORMAT_SIGNED = 0x100;
    public static final int FORMAT_BIG_ENDIAN = 0x200;

    public static final int STATE_INITIAL = 0;
    public static final int STATE_PLAYING = 1;
    public static final int STATE_PAUSED = 2;
    public static final int STATE_STOPPED = 3;
    public static final int GENERATION_MASK = 0xFFFFFF;

    private SoundAbi() {}
}
