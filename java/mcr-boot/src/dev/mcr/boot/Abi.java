package dev.mcr.boot;

/** Mirror of mcr-jvm's native table and ring layout. Entries are append-only; Rust tests compare both lists. */
public final class Abi {
    public static final int MAGIC = 0x4D435242;
    public static final int VERSION = 1;
    public static final int TABLE_HEADER_SIZE = 16;

    public static final int RING_MAGIC = 0x4D435252;
    public static final int RING_HEADER_SIZE = 192;
    public static final int RING_WRITE_OFFSET = 64;
    public static final int RING_READ_OFFSET = 128;
    public static final int RECORD_HEADER_SIZE = 8;
    public static final int PADDING_KIND = 0xFFFF;

    public enum Flavour { CRITICAL, NORMAL }

    public enum Entry {
        REGISTER_UPCALLS("register_upcalls", "(JI)I", Flavour.NORMAL),
        HOST_CALL("host_call", "(IJ)J", Flavour.NORMAL),
        DOORBELL_WAIT("doorbell_wait", "(JIJ)I", Flavour.NORMAL),
        DOORBELL_NOTIFY("doorbell_notify", "(J)V", Flavour.CRITICAL),
        NOP_CRITICAL("nop_critical", "(J)J", Flavour.CRITICAL),
        NOP_NORMAL("nop_normal", "(J)J", Flavour.NORMAL);

        public final String symbol;
        public final String descriptor;
        public final Flavour flavour;

        Entry(String symbol, String descriptor, Flavour flavour) {
            this.symbol = symbol;
            this.descriptor = descriptor;
            this.flavour = flavour;
        }
    }

    public enum Upcall {
        SELFTEST_PING("selftest_ping", "()I"),
        SELFTEST_ADD("selftest_add", "(JJ)I"),
        SELFTEST_THROW("selftest_throw", "()I");

        public final String symbol;
        public final String descriptor;

        Upcall(String symbol, String descriptor) {
            this.symbol = symbol;
            this.descriptor = descriptor;
        }
    }

    private Abi() {}
}
