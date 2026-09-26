package dev.hynergy.electrical.internal;

import java.lang.foreign.MemoryLayout;
import java.lang.foreign.ValueLayout;

public final class NativeLayouts {
    static final int REQUIRED_ABI_VERSION = 4;
    static final int REQUIRED_ABI_REVISION = 0;

    public static final int WORLD_SUCCESS = 0;
    public static final int WORLD_NULL_ENGINE = 1;
    public static final int WORLD_NULL_RESULT = 2;
    public static final int WORLD_INVALID_TICK_FREQUENCY = 5;
    public static final int WORLD_INTERNAL_PANIC = -1;

    public static final MemoryLayout COMMAND_RESULT =
            MemoryLayout.structLayout(
                    ValueLayout.JAVA_INT.withName("code"),
                    ValueLayout.JAVA_INT.withName("command_index"),
                    ValueLayout.JAVA_INT.withName("byte_offset"),
                    ValueLayout.JAVA_INT.withName("reserved")
            );

    public static final long COMMAND_RESULT_COMMAND_INDEX_OFFSET =
            offsetOf(COMMAND_RESULT, "command_index");

    public static final long COMMAND_RESULT_BYTE_OFFSET =
            offsetOf(COMMAND_RESULT, "byte_offset");

    public static void verifyAbi() {
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

    private static long offsetOf(
            MemoryLayout layout,
            String member
    ) {
        return layout.byteOffset(
                MemoryLayout.PathElement.groupElement(member)
        );
    }
}
