package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;
import dev.hynergy.electrical.internal.NativeLayouts;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;

public final class ElectricalEngine implements AutoCloseable {
    private MemorySegment handle;

    private static final AtomicBoolean engineAlreadyInstantiated = new AtomicBoolean(false);
    private final AtomicBoolean definitionRegistrationPoisoned = new AtomicBoolean(false);

    private ElectricalEngine(MemorySegment handle) {
        this.handle = handle;
    }

    public static ElectricalEngine create() {
        if (!engineAlreadyInstantiated.compareAndSet(false, true)) {
            throw new IllegalStateException("Engine already instantiated");
        }

        MemorySegment handle;

        try {
            handle = NativeBindings.createEngine(1);
        } catch (RuntimeException | Error failure) {
            engineAlreadyInstantiated.set(false);
            throw failure;
        }

        if (MemorySegment.NULL.equals(handle)) {
            engineAlreadyInstantiated.set(false);

            throw new IllegalStateException("Native electrical engine creation returned a null handle");
        }

        return new ElectricalEngine(handle);
    }

    /**
     * Registers a device definition.
     *
     * <p>The method does not reset the builder. The caller can reset and reuse
     * the builder after this method returns.</p>
     *
     * @param builder the builder that contains the device definition
     *
     * @return the registered device definition
     *
     * @throws IllegalArgumentException if the device definition is not valid
     * @throws IllegalStateException if registration cannot continue
     */
    public DeviceDefinition registerDefinition(
        DeviceDefinitionBuilder builder
    ) {
        Objects.requireNonNull(builder, "builder");

        MemorySegment engine = requireOpen();

        if (definitionRegistrationPoisoned.get()) {
            throw new IllegalStateException(
                "Device definition registration is unavailable after an unrecoverable failure");
        }

        MemorySegment input = builder.encodedSegment();
        int inputLength = builder.encodedLength();

        try (Arena arena = Arena.ofConfined()) {
            MemorySegment result = arena.allocate(NativeLayouts.DEFINITION_REGISTRATION_RESULT);

            final int code;

            try {
                code = NativeBindings.registerDefinition(engine, input, inputLength, result);
            } catch (RuntimeException | Error failure) {
                definitionRegistrationPoisoned.set(true);
                throw failure;
            }

            if (code == DefinitionRegistrationCode.SUCCESS) {
                int definitionId =
                    result.get(ValueLayout.JAVA_INT, NativeLayouts.DEFINITION_REGISTRATION_RESULT_DEFINITION_ID_OFFSET);

                if (definitionId == 0) {
                    definitionRegistrationPoisoned.set(true);

                    throw new IllegalStateException(
                        "Native definition registration succeeded with an invalid definition ID");
                }

                return new DeviceDefinition(definitionId);
            }

            int commandIndex =
                result.get(ValueLayout.JAVA_INT, NativeLayouts.DEFINITION_REGISTRATION_RESULT_COMMAND_INDEX_OFFSET);

            int byteOffset = result.get(ValueLayout.JAVA_INT, NativeLayouts.DEFINITION_REGISTRATION_RESULT_BYTE_OFFSET);

            throw registrationFailure(code, commandIndex, byteOffset);
        }
    }

    private RuntimeException registrationFailure(int code, int commandIndex, int byteOffset) {
        String details =
            "code=" + Integer.toUnsignedLong(code) + ", commandIndex=" + formatOptionalUnsigned(commandIndex)
                + ", byteOffset=" + formatOptionalUnsigned(byteOffset);

        return switch (code) {
            case DefinitionRegistrationCode.UNKNOWN_DEFINITION ->
                new IllegalArgumentException("Device definition references an unknown definition: " + details);

            case DefinitionRegistrationCode.TERMINAL_COUNT_MISMATCH ->
                new IllegalArgumentException("Device definition has an invalid terminal count: " + details);

            case DefinitionRegistrationCode.PARAMETER_COUNT_MISMATCH ->
                new IllegalArgumentException("Device definition has an invalid parameter count: " + details);

            case DefinitionRegistrationCode.NODE_OUT_OF_RANGE ->
                new IllegalArgumentException("Device definition references an invalid node: " + details);

            case DefinitionRegistrationCode.PARAMETER_OUT_OF_RANGE ->
                new IllegalArgumentException("Device definition references an invalid parameter: " + details);

            case DefinitionRegistrationCode.PARAMETER_CONSTRAINT_VIOLATION ->
                new IllegalArgumentException("Device definition violates a parameter constraint: " + details);

            case DefinitionRegistrationCode.INVALID_DEFINITION ->
                new IllegalArgumentException("Device definition is not valid: " + details);

            case DefinitionRegistrationCode.INVALID_PRIMITIVE_PARAMETERS ->
                new IllegalArgumentException("Device definition has invalid primitive parameters: " + details);

            case DefinitionRegistrationCode.UNUSED_INTERNAL_NODE ->
                new IllegalArgumentException("Device definition contains an unused internal node: " + details);

            case DefinitionRegistrationCode.INCOMPATIBLE_PARAMETER_CONSTRAINTS ->
                new IllegalArgumentException("Device definition has incompatible parameter constraints: " + details);

            case DefinitionRegistrationCode.UNUSED_PARAMETER ->
                new IllegalArgumentException("Device definition contains an unused parameter: " + details);

            case DefinitionRegistrationCode.NODE_ID_EXHAUSTED, DefinitionRegistrationCode.PARAMETER_ID_EXHAUSTED,
                 DefinitionRegistrationCode.DEFINITION_ID_EXHAUSTED,
                 DefinitionRegistrationCode.DEVICE_PARTITION_ID_EXHAUSTED,
                 DefinitionRegistrationCode.STATE_COUNT_EXHAUSTED ->
                new IllegalStateException("Native definition ID capacity is exhausted: " + details);

            case DefinitionRegistrationCode.INVALID_MAGIC, DefinitionRegistrationCode.UNSUPPORTED_VERSION,
                 DefinitionRegistrationCode.INVALID_FLAGS, DefinitionRegistrationCode.TRUNCATED_INPUT,
                 DefinitionRegistrationCode.UNKNOWN_COMMAND, DefinitionRegistrationCode.INVALID_COMMAND_LENGTH,
                 DefinitionRegistrationCode.INVALID_COUNT, DefinitionRegistrationCode.UNKNOWN_VALUE_KIND,
                 DefinitionRegistrationCode.TRAILING_BYTES, DefinitionRegistrationCode.INVALID_RESERVED,
                 DefinitionRegistrationCode.INVALID_DEFINITION_ID -> new IllegalStateException(
                "DeviceDefinitionBuilder produced an invalid native definition buffer: " + details);

            case DefinitionRegistrationCode.NULL_ENGINE, DefinitionRegistrationCode.NULL_INPUT,
                 DefinitionRegistrationCode.NULL_RESULT -> {
                definitionRegistrationPoisoned.set(true);

                yield new IllegalStateException(
                    "Native definition registration reported an invalid FFI argument: " + details);
            }

            case DefinitionRegistrationCode.INTERNAL_PANIC -> {
                definitionRegistrationPoisoned.set(true);

                yield new IllegalStateException("Native engine panicked during device definition registration");
            }

            default -> {
                definitionRegistrationPoisoned.set(true);

                yield new IllegalStateException(
                    "Unknown native definition registration status: " + Integer.toUnsignedLong(code));
            }
        };
    }

    private static String formatOptionalUnsigned(int value) {
        if (value == -1) {
            return "none";
        }

        return Long.toString(Integer.toUnsignedLong(value));
    }

    public ElectricalWorld createWorld(int tickFrequencyHz) {
        if (tickFrequencyHz <= 0) {
            throw new IllegalArgumentException("Tick frequency must be greater than zero");
        }

        MemorySegment worldHandle;

        try (Arena arena = Arena.ofConfined()) {
            MemorySegment worldResult = arena.allocate(ValueLayout.ADDRESS);

            int code = NativeBindings.createWorld(requireOpen(), tickFrequencyHz, worldResult);

            switch (code) {
                case WorldCode.SUCCESS -> {
                }
                case WorldCode.NULL_ENGINE ->
                    throw new IllegalStateException("Native ABI reported a null engine handle");

                case WorldCode.NULL_RESULT ->
                    throw new IllegalStateException("Native ABI reported a null world output pointer");

                case WorldCode.INVALID_TICK_FREQUENCY ->
                    throw new IllegalStateException("Native ABI rejected a validated tick frequency");

                case WorldCode.INTERNAL_PANIC ->
                    throw new IllegalStateException("Native engine panicked while creating a world");

                default -> throw new IllegalStateException(
                    "Unknown native world creation status: " + Integer.toUnsignedLong(code));
            }

            worldHandle = worldResult.get(ValueLayout.ADDRESS, 0);

            if (MemorySegment.NULL.equals(worldHandle)) {
                throw new IllegalStateException("Native world creation succeeded with a null " + "handle");
            }
        }

        try {
            return new ElectricalWorld(worldHandle);
        } catch (RuntimeException | Error failure) {
            try {
                NativeBindings.destroyWorld(worldHandle);
            } catch (RuntimeException | Error destroyFailure) {
                failure.addSuppressed(destroyFailure);
            }

            throw failure;
        }
    }

    MemorySegment requireOpen() {
        if (handle.equals(MemorySegment.NULL)) {
            throw new IllegalStateException("Electrical engine is closed");
        }

        return handle;
    }

    @Override
    public void close() {
        if (MemorySegment.NULL.equals(handle)) {
            return;
        }

        NativeBindings.destroyEngine(handle);

        handle = MemorySegment.NULL;
        engineAlreadyInstantiated.set(false);
    }



    private static final class WorldCode {
        static final int SUCCESS = 0;
        static final int NULL_ENGINE = 1;
        static final int NULL_RESULT = 2;
        static final int INVALID_TICK_FREQUENCY = 5;
        static final int INTERNAL_PANIC = -1;
    }

    private static final class DefinitionRegistrationCode {
        static final int SUCCESS = 0;

        static final int NULL_ENGINE = 1;
        static final int NULL_INPUT = 2;
        static final int NULL_RESULT = 3;

        static final int INVALID_MAGIC = 5;
        static final int UNSUPPORTED_VERSION = 6;
        static final int INVALID_FLAGS = 7;
        static final int TRUNCATED_INPUT = 8;
        static final int UNKNOWN_COMMAND = 9;
        static final int INVALID_COMMAND_LENGTH = 10;
        static final int INVALID_COUNT = 11;
        static final int UNKNOWN_VALUE_KIND = 12;
        static final int TRAILING_BYTES = 13;
        static final int INVALID_RESERVED = 14;
        static final int INVALID_DEFINITION_ID = 15;

        static final int UNKNOWN_DEFINITION = 20;
        static final int TERMINAL_COUNT_MISMATCH = 21;
        static final int PARAMETER_COUNT_MISMATCH = 22;
        static final int NODE_OUT_OF_RANGE = 23;
        static final int PARAMETER_OUT_OF_RANGE = 24;
        static final int PARAMETER_CONSTRAINT_VIOLATION = 25;
        static final int NODE_ID_EXHAUSTED = 26;
        static final int PARAMETER_ID_EXHAUSTED = 27;
        static final int DEFINITION_ID_EXHAUSTED = 28;
        static final int INVALID_DEFINITION = 29;
        static final int INVALID_PRIMITIVE_PARAMETERS = 30;
        static final int UNUSED_INTERNAL_NODE = 31;
        static final int INCOMPATIBLE_PARAMETER_CONSTRAINTS = 33;
        static final int UNUSED_PARAMETER = 34;
        static final int DEVICE_PARTITION_ID_EXHAUSTED = 35;
        static final int STATE_COUNT_EXHAUSTED = 36;

        static final int INTERNAL_PANIC = -1;
    }
}
