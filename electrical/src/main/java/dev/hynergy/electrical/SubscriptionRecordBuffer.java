package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeLayouts;
import org.jspecify.annotations.Nullable;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

final class SubscriptionRecordBuffer implements AutoCloseable {
    private static final long RECORD_SIZE = NativeLayouts.SUBSCRIPTION_RECORD.byteSize();

    private @Nullable Arena arena;
    private MemorySegment records = MemorySegment.NULL;
    private int capacity;

    private boolean closed;

    MemorySegment segment() {
        requireOpen();
        return records;
    }

    int capacity() {
        requireOpen();
        return capacity;
    }

    void ensureCapacity(int requiredCapacity) {
        requireOpen();

        if (requiredCapacity < 0) {
            throw new IllegalArgumentException("Required capacity must be non-negative");
        }

        if (requiredCapacity <= capacity) {
            return;
        }

        int newCapacity = growCapacity(capacity, requiredCapacity);
        Arena newArena = Arena.ofConfined();

        final MemorySegment newRecords;

        try {
            newRecords = newArena.allocate(NativeLayouts.SUBSCRIPTION_RECORD, newCapacity);
        } catch (RuntimeException | Error failure) {
            closeAfterFailure(newArena, failure);
            throw failure;
        }

        Arena oldArena = arena;

        if (oldArena != null) {
            try {
                oldArena.close();
            } catch (RuntimeException | Error failure) {
                closeAfterFailure(newArena, failure);
                throw failure;
            }
        }

        arena = newArena;
        records = newRecords;
        capacity = newCapacity;
    }

    private static int growCapacity(int currentCapacity, int requiredCapacity) {
        int capacity = Math.max(currentCapacity, 8);

        while (capacity < requiredCapacity) {
            int next = capacity + (capacity >>> 1);

            if (next <= capacity) {
                return requiredCapacity;
            }

            capacity = next;
        }

        return capacity;
    }

    int subscriptionIdAt(int index) {
        return records.get(ValueLayout.JAVA_INT, recordOffset(index) + NativeLayouts.SUBSCRIPTION_RECORD_ID_OFFSET);
    }

    int subscriptionStatusAt(int index) {
        return records.get(ValueLayout.JAVA_INT, recordOffset(index) + NativeLayouts.SUBSCRIPTION_RECORD_STATUS_OFFSET);
    }

    double subscriptionValueAt(int index) {
        return records.get(
            ValueLayout.JAVA_DOUBLE,
            recordOffset(index) + NativeLayouts.SUBSCRIPTION_RECORD_VALUE_OFFSET
        );
    }

    private long recordOffset(int index) {
        requireOpen();

        if (index < 0 || index >= capacity) {
            throw new IndexOutOfBoundsException("Subscription record index out of bounds: " + index);
        }

        return (long) index * RECORD_SIZE;
    }

    private static void closeAfterFailure(Arena arena, Throwable failure) {
        try {
            arena.close();
        } catch (RuntimeException | Error closeFailure) {
            failure.addSuppressed(closeFailure);
        }
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Subscription record buffer is closed");
        }
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }

        Arena arena = this.arena;

        if (arena != null) {
            arena.close();
        }

        closed = true;
    }
}