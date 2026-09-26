package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;
import dev.hynergy.electrical.internal.NativeLayouts;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;


public final class ElectricalEngine implements AutoCloseable {
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
            throw new IllegalArgumentException(
                    "Tick frequency must be greater than zero"
            );
        }

        try (Arena arena = Arena.ofConfined()) {
            MemorySegment worldResult =
                    arena.allocate(ValueLayout.ADDRESS);

            int code = NativeBindings.createWorld(
                    requireOpen(),
                    tickFrequencyHz,
                    worldResult
            );

            switch (code) {
                case NativeLayouts.WORLD_SUCCESS -> {
                }
                case NativeLayouts.WORLD_NULL_ENGINE -> throw new IllegalStateException(
                        "Native engine handle is null"
                );
                case NativeLayouts.WORLD_NULL_RESULT -> throw new IllegalStateException(
                        "Native world result pointer is null"
                );
                case NativeLayouts.WORLD_INVALID_TICK_FREQUENCY -> throw new IllegalStateException(
                        "Native engine rejected the tick frequency"
                );
                case NativeLayouts.WORLD_INTERNAL_PANIC -> throw new IllegalStateException(
                        "Native engine panicked while creating a world"
                );
                default -> throw new IllegalStateException(
                        "Unknown native world creation status: "
                                + Integer.toUnsignedLong(code)
                );
            }

            MemorySegment worldHandle = worldResult.get(
                    ValueLayout.ADDRESS,
                    0
            );

            if (worldHandle.equals(MemorySegment.NULL)) {
                throw new IllegalStateException(
                        "Native world creation succeeded with a null handle"
                );
            }

            return new ElectricalWorld(worldHandle);
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
