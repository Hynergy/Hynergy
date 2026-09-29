package dev.hynergy.electrical.internal;

final class NativeAbi {
    private static final int REQUIRED_VERSION = 6;
    private static final int REQUIRED_REVISION = 0;

    private NativeAbi() {
    }

    static void verify() {
        int version = NativeBindings.abiVersion();
        int revision = NativeBindings.abiRevision();

        if (version != REQUIRED_VERSION) {
            throw new IllegalStateException(
                "Native ABI version mismatch. Expected " + REQUIRED_VERSION + ", got " + version);
        }

        if (revision < REQUIRED_REVISION) {
            throw new IllegalStateException(
                "Native ABI revision is too old. Required at least " + REQUIRED_REVISION + ", got " + revision);
        }
    }
}
