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



    public static final MemoryLayout SUBSCRIPTION_RECORD = MemoryLayout.structLayout(
        ValueLayout.JAVA_INT.withName("subscription_id"),
        ValueLayout.JAVA_INT.withName("status"),
        ValueLayout.JAVA_DOUBLE.withName("value")
    );

    public static final long SUBSCRIPTION_RECORD_ID_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "subscription_id");
    public static final long SUBSCRIPTION_RECORD_STATUS_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "status");
    public static final long SUBSCRIPTION_RECORD_VALUE_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "value");



    public static final MemoryLayout TICK_RESULT = MemoryLayout.structLayout(
        ValueLayout.JAVA_INT.withName("record_count"),
        ValueLayout.JAVA_INT.withName("required_capacity"),

        ValueLayout.JAVA_INT.withName("device_id"),
        ValueLayout.JAVA_INT.withName("parameter_id"),
        ValueLayout.JAVA_INT.withName("iterations")
    );

    public static final long TICK_RESULT_RECORD_COUNT_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "record_count");
    public static final long TICK_RESULT_REQUIRED_CAPACITY_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "required_capacity");
    public static final long TICK_RESULT_DEVICE_ID_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "device_id");
    public static final long TICK_RESULT_PARAMETER_ID_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "parameter_id");
    public static final long TICK_RESULT_ITERATIONS_OFFSET = offsetOf(SUBSCRIPTION_RECORD, "iterations");



    private static long offsetOf(MemoryLayout layout, String member) {
        return layout.byteOffset(MemoryLayout.PathElement.groupElement(member));
    }
}
