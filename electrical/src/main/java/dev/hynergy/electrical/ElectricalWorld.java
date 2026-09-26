package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;

import java.lang.foreign.MemorySegment;

public final class ElectricalWorld implements AutoCloseable {
    private MemorySegment handle;

    ElectricalWorld(MemorySegment handle) {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalArgumentException(
                    "World handle must not be null"
            );
        }

        this.handle = handle;
    }

    MemorySegment requireOpen() {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalStateException(
                    "Electrical world is closed"
            );
        }

        return handle;
    }

    @Override
    public void close() {
        if (!MemorySegment.NULL.equals(handle)) {
            NativeBindings.destroyWorld(handle);
            handle = MemorySegment.NULL;
        }
    }
}
