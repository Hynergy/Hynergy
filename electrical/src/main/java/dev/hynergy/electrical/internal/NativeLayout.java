package dev.hynergy.electrical.internal;

final class NativeLayout {
    static final int REQUIRED_ABI_VERSION = 3;
    static final int REQUIRED_ABI_REVISION = 0;


    static void verifyAbi() {
        int version = NativeBindings.abiVersion();
        int revision = NativeBindings.abiRevision();

        if (version != REQUIRED_ABI_VERSION) {
            throw new IllegalStateException(
                    "ABI version mismatch. Expected: "
                            + REQUIRED_ABI_VERSION
                            + ", actual: "
                            + version
            );
        }

        if (revision < REQUIRED_ABI_REVISION) {
            throw new IllegalStateException(
                    "ABI revision mismatch. Minimum required: "
                            + REQUIRED_ABI_REVISION
                            + ", actual: "
                            + revision
            );
        }
    }
}
