package dev.hynergy.electrical.internal;

import java.lang.foreign.MemoryLayout;
import java.lang.foreign.ValueLayout;

public final class NativeLayouts {

    public static final MemoryLayout DEFINITION_REGISTRATION_RESULT = MemoryLayout.structLayout(
        ValueLayout.JAVA_INT.withName("command_index"),
        ValueLayout.JAVA_INT.withName("byte_offset"),
        ValueLayout.JAVA_INT.withName("definition_id")
    );

    public static final long DEFINITION_REGISTRATION_RESULT_COMMAND_INDEX_OFFSET =
        offsetOf(DEFINITION_REGISTRATION_RESULT, "command_index");

    public static final long DEFINITION_REGISTRATION_RESULT_BYTE_OFFSET =
        offsetOf(DEFINITION_REGISTRATION_RESULT, "byte_offset");

    public static final long DEFINITION_REGISTRATION_RESULT_DEFINITION_ID_OFFSET =
        offsetOf(DEFINITION_REGISTRATION_RESULT, "definition_id");

    public static final MemoryLayout COMMAND_RESULT = MemoryLayout.structLayout(
        ValueLayout.JAVA_INT.withName("command_index"),
        ValueLayout.JAVA_INT.withName("byte_offset"),
        ValueLayout.JAVA_INT.withName("reserved")
    );

    public static final long COMMAND_RESULT_COMMAND_INDEX_OFFSET = offsetOf(COMMAND_RESULT, "command_index");
    public static final long COMMAND_RESULT_BYTE_OFFSET = offsetOf(COMMAND_RESULT, "byte_offset");

    private static long offsetOf(MemoryLayout layout, String member) {
        return layout.byteOffset(MemoryLayout.PathElement.groupElement(member));
    }
}
