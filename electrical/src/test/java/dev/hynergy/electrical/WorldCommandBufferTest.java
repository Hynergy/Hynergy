package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

import static org.junit.jupiter.api.Assertions.*;

final class WorldCommandBufferTest {
    private static final ValueLayout.OfInt U32_LE =
            ValueLayout.JAVA_INT_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);

    @Test
    void encodesEveryWorldCommand() {
        try (WorldCommandBuffer buffer = new WorldCommandBuffer()) {
            buffer.addWire(1);
            buffer.removeWire(2);
            buffer.connectWires(3, 4);
            buffer.disconnectWires(5, 6);
            buffer.addDevice(7, 8);
            buffer.removeDevice(9);
            buffer.attachTerminal(10, 11, 12);
            buffer.detachTerminal(13, 14, 15);
            buffer.setDeviceParameter(16, 17, 18.5);

            ByteBuffer expected = ByteBuffer.allocate(146)
                    .order(ByteOrder.LITTLE_ENDIAN);

            expected.put((byte) 'H');
            expected.put((byte) 'Y');
            expected.put((byte) 'W');
            expected.put((byte) 'C');
            expected.putShort((short) 1);
            expected.putShort((short) 0);
            expected.putInt(0);
            expected.putInt(9);

            command(expected, 1, 4).putInt(1);
            command(expected, 2, 4).putInt(2);
            command(expected, 3, 8).putInt(3).putInt(4);
            command(expected, 4, 8).putInt(5).putInt(6);
            command(expected, 5, 8).putInt(7).putInt(8);
            command(expected, 6, 4).putInt(9);
            command(expected, 7, 12).putInt(10).putInt(11).putInt(12);
            command(expected, 8, 12).putInt(13).putInt(14).putInt(15);
            command(expected, 9, 16).putInt(16).putInt(17).putDouble(18.5);

            assertEquals(9, buffer.commandCount());
            assertEquals(146, buffer.byteSize());
            assertArrayEquals(expected.array(), bytes(buffer));
        }
    }

    @Test
    void growthReplacesAndReleasesTheOldBackingArena() {
        try (WorldCommandBuffer buffer = new WorldCommandBuffer(16)) {
            MemorySegment oldSegment = buffer.segmentForApply();

            assertEquals(0, oldSegment.address() % 8);

            buffer.setDeviceParameter(1, 0, 1.0);

            assertEquals(64, buffer.capacity());
            assertEquals(0, buffer.segmentForApply().address() % 8);
            assertThrows(
                    IllegalStateException.class,
                    () -> oldSegment.get(ValueLayout.JAVA_BYTE, 0)
            );
        }
    }

    @Test
    void clearRetainsCapacityAndResetsTheBatch() {
        try (WorldCommandBuffer buffer = new WorldCommandBuffer(16)) {
            buffer.setDeviceParameter(1, 0, 2.0);
            int capacity = buffer.capacity();

            buffer.clear();

            assertTrue(buffer.isEmpty());
            assertEquals(0, buffer.commandCount());
            assertEquals(16, buffer.byteSize());
            assertEquals(capacity, buffer.capacity());

            byte[] bytes = bytes(buffer);
            assertEquals(16, bytes.length);
            assertEquals(
                    0,
                    ByteBuffer.wrap(bytes)
                            .order(ByteOrder.LITTLE_ENDIAN)
                            .getInt(12)
            );
        }
    }

    @Test
    void commandCountIsWrittenOnlyWhenPreparedForApply() {
        try (WorldCommandBuffer buffer = new WorldCommandBuffer()) {
            MemorySegment segment = buffer.segmentForApply();
            assertEquals(0, segment.get(U32_LE, 12));

            buffer.addWire(1);

            assertEquals(0, segment.get(U32_LE, 12));

            buffer.segmentForApply();

            assertEquals(1, segment.get(U32_LE, 12));
        }
    }

    @Test
    void invalidArgumentsDoNotMutateTheBatch() {
        try (WorldCommandBuffer buffer = new WorldCommandBuffer()) {
            int byteSize = buffer.byteSize();
            int commandCount = buffer.commandCount();

            assertThrows(IllegalArgumentException.class, () -> buffer.addWire(0));
            assertThrows(IllegalArgumentException.class, () -> buffer.removeDevice(-1));
            assertThrows(IllegalArgumentException.class, () -> buffer.addDevice(1, 0));
            assertThrows(
                    IllegalArgumentException.class,
                    () -> buffer.attachTerminal(1, 1, -1)
            );
            assertThrows(
                    IllegalArgumentException.class,
                    () -> buffer.setDeviceParameter(1, -1, 1.0)
            );

            assertEquals(byteSize, buffer.byteSize());
            assertEquals(commandCount, buffer.commandCount());
        }
    }

    @Test
    void closeIsIdempotentAndInvalidatesFurtherUse() {
        WorldCommandBuffer buffer = new WorldCommandBuffer();

        buffer.close();
        buffer.close();

        assertThrows(IllegalStateException.class, buffer::clear);
        assertThrows(IllegalStateException.class, buffer::byteSize);
        assertThrows(IllegalStateException.class, () -> buffer.addWire(1));
        assertThrows(IllegalStateException.class, buffer::segmentForApply);
    }

    @Test
    void constructorValidatesAndUsesTheDefaultCapacity() {
        assertThrows(IllegalArgumentException.class, () -> new WorldCommandBuffer(15));

        try (WorldCommandBuffer buffer = new WorldCommandBuffer()) {
            assertEquals(256, buffer.capacity());
            assertEquals(16, buffer.byteSize());
            assertTrue(buffer.isEmpty());
        }
    }

    private static ByteBuffer command(ByteBuffer buffer, int tag, int payloadLength) {
        return buffer
                .putShort((short) tag)
                .putInt(payloadLength);
    }

    private static byte[] bytes(WorldCommandBuffer buffer) {
        MemorySegment segment = buffer.segmentForApply();
        int byteSize = buffer.byteSize();
        byte[] bytes = new byte[byteSize];

        MemorySegment.copy(
                segment,
                ValueLayout.JAVA_BYTE,
                0,
                bytes,
                0,
                byteSize
        );

        return bytes;
    }
}
