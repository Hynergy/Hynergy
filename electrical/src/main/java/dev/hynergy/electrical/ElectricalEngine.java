package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public final class ElectricalEngine implements AutoCloseable {
    private static final class WorldCode {
        static final int SUCCESS = 0;
        static final int NULL_ENGINE = 1;
        static final int NULL_RESULT = 2;
        static final int INVALID_TICK_FREQUENCY = 5;
        static final int INTERNAL_PANIC = -1;
    }

    private MemorySegment handle;

    private ElectricalEngine(MemorySegment handle) {
        this.handle = handle;
    }

    public static ElectricalEngine create() {
        MemorySegment handle = NativeBindings.createEngine(1);

        if (handle.equals(MemorySegment.NULL)) {
            throw new IllegalStateException("Native electrical engine creation returned a null handle");
        }

        return new ElectricalEngine(handle);
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
        if (!MemorySegment.NULL.equals(handle)) {
            NativeBindings.destroyEngine(handle);
            handle = MemorySegment.NULL;
        }
    }
}
