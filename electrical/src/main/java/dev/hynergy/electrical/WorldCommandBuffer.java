package dev.hynergy.electrical;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.ByteOrder;


final class WorldCommandBuffer implements AutoCloseable {
    private static final int DEFAULT_CAPACITY = 256;
    private static final int HEADER_SIZE = 16;
    private static final int COMMAND_HEADER_SIZE = 6;
    private static final int BUFFER_ALIGNMENT = 8;
    private static final int MAX_CAPACITY = Integer.MAX_VALUE;

    private static final int COMMAND_COUNT_OFFSET = 12;

    private static final short VERSION = 1;

    private static final short ADD_WIRE = 1;
    private static final short REMOVE_WIRE = 2;
    private static final short CONNECT_WIRES = 3;
    private static final short DISCONNECT_WIRES = 4;
    private static final short ADD_DEVICE = 5;
    private static final short REMOVE_DEVICE = 6;
    private static final short ATTACH_TERMINAL = 7;
    private static final short DETACH_TERMINAL = 8;
    private static final short SET_DEVICE_PARAMETER = 9;

    private static final ValueLayout.OfShort U16_LE =
            ValueLayout.JAVA_SHORT_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);
    private static final ValueLayout.OfInt U32_LE =
            ValueLayout.JAVA_INT_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);
    private static final ValueLayout.OfDouble F64_LE =
            ValueLayout.JAVA_DOUBLE_UNALIGNED.withOrder(ByteOrder.LITTLE_ENDIAN);

    private Arena arena;
    private MemorySegment segment;
    private int capacity;
    private int position;
    private int commandCount;

    WorldCommandBuffer() {
        this(DEFAULT_CAPACITY);
    }

    WorldCommandBuffer(int initialCapacity) {
        if (initialCapacity < HEADER_SIZE) {
            throw new IllegalArgumentException(
                    "Initial capacity must be at least " + HEADER_SIZE + " bytes"
            );
        }

        arena = Arena.ofConfined();
        try {
            segment = arena.allocate(initialCapacity, BUFFER_ALIGNMENT);
        } catch (Throwable throwable) {
            arena.close();
            throw throwable;
        }

        capacity = initialCapacity;
        position = HEADER_SIZE;
        commandCount = 0;
        initializeHeader(segment);
    }

    int commandCount() {
        requireOpen();
        return commandCount;
    }

    int byteSize() {
        requireOpen();
        return position;
    }

    int capacity() {
        requireOpen();
        return capacity;
    }

    boolean isEmpty() {
        requireOpen();
        return commandCount == 0;
    }

    void clear() {
        requireOpen();
        position = HEADER_SIZE;
        commandCount = 0;
    }

    void addWire(int wireId) {
        requirePositiveId(wireId, "wireId");

        int payload = beginCommand(ADD_WIRE, 4);
        writeU32(payload, wireId);
        finishCommand(4);
    }

    void removeWire(int wireId) {
        requirePositiveId(wireId, "wireId");

        int payload = beginCommand(REMOVE_WIRE, 4);
        writeU32(payload, wireId);
        finishCommand(4);
    }

    void connectWires(int wireAId, int wireBId) {
        requirePositiveId(wireAId, "wireAId");
        requirePositiveId(wireBId, "wireBId");

        int payload = beginCommand(CONNECT_WIRES, 8);
        writeU32(payload, wireAId);
        writeU32(payload + 4, wireBId);
        finishCommand(8);
    }

    void disconnectWires(int wireAId, int wireBId) {
        requirePositiveId(wireAId, "wireAId");
        requirePositiveId(wireBId, "wireBId");

        int payload = beginCommand(DISCONNECT_WIRES, 8);
        writeU32(payload, wireAId);
        writeU32(payload + 4, wireBId);
        finishCommand(8);
    }

    void addDevice(int deviceId, int definitionId) {
        requirePositiveId(deviceId, "deviceId");
        requirePositiveId(definitionId, "definitionId");

        int payload = beginCommand(ADD_DEVICE, 8);
        writeU32(payload, deviceId);
        writeU32(payload + 4, definitionId);
        finishCommand(8);
    }

    void removeDevice(int deviceId) {
        requirePositiveId(deviceId, "deviceId");

        int payload = beginCommand(REMOVE_DEVICE, 4);
        writeU32(payload, deviceId);
        finishCommand(4);
    }

    void attachTerminal(int wireId, int deviceId, int terminalId) {
        requirePositiveId(wireId, "wireId");
        requirePositiveId(deviceId, "deviceId");
        requireIndex(terminalId, "terminalId");

        int payload = beginCommand(ATTACH_TERMINAL, 12);
        writeU32(payload, wireId);
        writeU32(payload + 4, deviceId);
        writeU32(payload + 8, terminalId);
        finishCommand(12);
    }

    void detachTerminal(int wireId, int deviceId, int terminalId) {
        requirePositiveId(wireId, "wireId");
        requirePositiveId(deviceId, "deviceId");
        requireIndex(terminalId, "terminalId");

        int payload = beginCommand(DETACH_TERMINAL, 12);
        writeU32(payload, wireId);
        writeU32(payload + 4, deviceId);
        writeU32(payload + 8, terminalId);
        finishCommand(12);
    }

    void setDeviceParameter(int deviceId, int parameterId, double value) {
        requirePositiveId(deviceId, "deviceId");
        requireIndex(parameterId, "parameterId");

        int payload = beginCommand(SET_DEVICE_PARAMETER, 16);
        writeU32(payload, deviceId);
        writeU32(payload + 4, parameterId);
        segment.set(F64_LE, payload + 8L, value);
        finishCommand(16);
    }

    MemorySegment segmentForApply() {
        requireOpen();
        segment.set(U32_LE, COMMAND_COUNT_OFFSET, commandCount);
        return segment;
    }

    @Override
    public void close() {
        Arena currentArena = arena;
        if (currentArena == null) {
            return;
        }

        currentArena.close();
        arena = null;
        segment = MemorySegment.NULL;
        capacity = 0;
        position = 0;
        commandCount = 0;
    }

    private int beginCommand(short tag, int payloadSize) {
        requireOpen();

        int commandSize = COMMAND_HEADER_SIZE + payloadSize;
        ensureCapacity(commandSize);

        int commandOffset = position;
        segment.set(U16_LE, commandOffset, tag);
        segment.set(U32_LE, commandOffset + 2L, payloadSize);
        return commandOffset + COMMAND_HEADER_SIZE;
    }

    private void finishCommand(int payloadSize) {
        position += COMMAND_HEADER_SIZE + payloadSize;
        commandCount++;
    }

    private void ensureCapacity(int additionalBytes) {
        long required = (long) position + additionalBytes;
        if (required <= capacity) {
            return;
        }
        if (required > MAX_CAPACITY) {
            throw new IllegalStateException(
                    "World command buffer exceeds maximum capacity of " + MAX_CAPACITY + " bytes"
            );
        }

        int newCapacity = capacity;
        while (newCapacity < required) {
            if (newCapacity > MAX_CAPACITY / 2) {
                newCapacity = (int) required;
                break;
            }
            newCapacity *= 2;
        }

        replaceBacking(newCapacity);
    }

    private void replaceBacking(int newCapacity) {
        Arena newArena = Arena.ofConfined();
        MemorySegment newSegment;

        try {
            newSegment = newArena.allocate(newCapacity, BUFFER_ALIGNMENT);
            MemorySegment.copy(segment, 0, newSegment, 0, position);
        } catch (Throwable throwable) {
            try {
                newArena.close();
            } catch (Throwable closeFailure) {
                throwable.addSuppressed(closeFailure);
            }
            throw throwable;
        }

        Arena oldArena = arena;
        arena = newArena;
        segment = newSegment;
        capacity = newCapacity;

        oldArena.close();
    }

    private static void initializeHeader(MemorySegment target) {
        target.set(ValueLayout.JAVA_BYTE, 0, (byte) 'H');
        target.set(ValueLayout.JAVA_BYTE, 1, (byte) 'Y');
        target.set(ValueLayout.JAVA_BYTE, 2, (byte) 'W');
        target.set(ValueLayout.JAVA_BYTE, 3, (byte) 'C');
        target.set(U16_LE, 4, VERSION);
        target.set(U16_LE, 6, (short) 0);
        target.set(U32_LE, 8, 0);
        target.set(U32_LE, COMMAND_COUNT_OFFSET, 0);
    }

    private void writeU32(long offset, int value) {
        segment.set(U32_LE, offset, value);
    }

    private static void requirePositiveId(int value, String name) {
        if (value <= 0) {
            throw new IllegalArgumentException(name + " must be greater than zero");
        }
    }

    private static void requireIndex(int value, String name) {
        if (value < 0) {
            throw new IllegalArgumentException(name + " must be non-negative");
        }
    }

    private void requireOpen() {
        if (arena == null) {
            throw new IllegalStateException("World command buffer is closed");
        }
    }
}
