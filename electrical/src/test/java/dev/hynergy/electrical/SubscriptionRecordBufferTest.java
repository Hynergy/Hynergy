package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeLayouts;
import org.junit.jupiter.api.Test;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

import static org.junit.jupiter.api.Assertions.*;

final class SubscriptionRecordBufferTest {

    @Test
    void startsEmptyWithoutAllocatingNativeStorage() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            assertEquals(0, buffer.capacity());
            assertSame(MemorySegment.NULL, buffer.segment());
        }
    }

    @Test
    void firstAllocationUsesMinimumCapacity() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(1);

            assertEquals(8, buffer.capacity());
            assertNotSame(MemorySegment.NULL, buffer.segment());
        }
    }

    @Test
    void capacityGrowsGeometrically() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(1);

            assertEquals(8, buffer.capacity());

            buffer.ensureCapacity(9);

            assertEquals(12, buffer.capacity());

            buffer.ensureCapacity(13);

            assertEquals(18, buffer.capacity());
        }
    }

    @Test
    void sufficientCapacityDoesNotReallocate() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(4);

            MemorySegment records = buffer.segment();
            int capacity = buffer.capacity();

            buffer.ensureCapacity(4);
            buffer.ensureCapacity(1);
            buffer.ensureCapacity(0);

            assertSame(records, buffer.segment());
            assertEquals(capacity, buffer.capacity());
        }
    }

    @Test
    void growingBufferReplacesAndClosesPreviousArena() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(1);

            MemorySegment oldRecords = buffer.segment();

            oldRecords.set(ValueLayout.JAVA_INT, NativeLayouts.SUBSCRIPTION_RECORD_ID_OFFSET, 123);

            buffer.ensureCapacity(9);

            MemorySegment newRecords = buffer.segment();

            assertNotSame(oldRecords, newRecords);

            assertThrows(
                IllegalStateException.class,
                () -> oldRecords.get(ValueLayout.JAVA_INT, NativeLayouts.SUBSCRIPTION_RECORD_ID_OFFSET)
            );
        }
    }

    @Test
    void indexedAccessorsReadNativeRecordLayout() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(2);

            int index = 1;

            long recordOffset = (long) index * NativeLayouts.SUBSCRIPTION_RECORD.byteSize();

            int subscriptionId = 0x8000_0001;
            int status = 1;
            double value = -12.5;

            MemorySegment records = buffer.segment();

            records.set(
                ValueLayout.JAVA_INT,
                recordOffset + NativeLayouts.SUBSCRIPTION_RECORD_ID_OFFSET,
                subscriptionId
            );

            records.set(ValueLayout.JAVA_INT, recordOffset + NativeLayouts.SUBSCRIPTION_RECORD_STATUS_OFFSET, status);
            records.set(ValueLayout.JAVA_DOUBLE, recordOffset + NativeLayouts.SUBSCRIPTION_RECORD_VALUE_OFFSET, value);

            assertEquals(subscriptionId, buffer.subscriptionIdAt(index));
            assertEquals(status, buffer.subscriptionStatusAt(index));
            assertEquals(value, buffer.subscriptionValueAt(index));
        }
    }

    @Test
    void indexedAccessorsRejectIndicesOutsideAllocatedCapacity() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(1);

            int capacity = buffer.capacity();

            assertThrows(IndexOutOfBoundsException.class, () -> buffer.subscriptionIdAt(-1));
            assertThrows(IndexOutOfBoundsException.class, () -> buffer.subscriptionStatusAt(capacity));
            assertThrows(IndexOutOfBoundsException.class, () -> buffer.subscriptionValueAt(capacity));
        }
    }

    @Test
    void negativeRequiredCapacityIsRejectedWithoutChangingBuffer() {
        try (SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer()) {
            buffer.ensureCapacity(1);

            MemorySegment records = buffer.segment();
            int capacity = buffer.capacity();

            assertThrows(IllegalArgumentException.class, () -> buffer.ensureCapacity(-1));

            assertSame(records, buffer.segment());
            assertEquals(capacity, buffer.capacity());
        }
    }

    @Test
    void closeIsIdempotentAndRejectsFurtherUse() {
        SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer();

        buffer.ensureCapacity(1);

        buffer.close();

        assertDoesNotThrow(buffer::close);

        assertThrows(IllegalStateException.class, buffer::segment);
        assertThrows(IllegalStateException.class, buffer::capacity);

        assertThrows(IllegalStateException.class, () -> buffer.ensureCapacity(1));
        assertThrows(IllegalStateException.class, () -> buffer.subscriptionIdAt(0));
        assertThrows(IllegalStateException.class, () -> buffer.subscriptionStatusAt(0));
        assertThrows(IllegalStateException.class, () -> buffer.subscriptionValueAt(0));
    }

    @Test
    void closeWithoutAllocationIsIdempotent() {
        SubscriptionRecordBuffer buffer = new SubscriptionRecordBuffer();

        buffer.close();

        assertDoesNotThrow(buffer::close);

        assertThrows(IllegalStateException.class, buffer::capacity);
    }
}