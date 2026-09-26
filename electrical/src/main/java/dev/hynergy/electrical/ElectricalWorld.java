package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;
import dev.hynergy.electrical.internal.NativeLayouts;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public final class ElectricalWorld implements AutoCloseable {
    private final WorldCommandBuffer commandBuffer;
    private final Arena scratchArena;
    private final MemorySegment commandResult;

    private MemorySegment handle;
    private boolean poisoned;

    ElectricalWorld(MemorySegment handle) {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalArgumentException(
                    "World handle must not be null"
            );
        }

        WorldCommandBuffer commandBuffer = null;
        Arena scratchArena = null;

        try {
            commandBuffer = new WorldCommandBuffer();

            scratchArena = Arena.ofConfined();
            this.commandResult =
                    scratchArena.allocate(NativeLayouts.COMMAND_RESULT);
        } catch (RuntimeException | Error failure) {
            if (scratchArena != null) {
                try {
                    scratchArena.close();
                } catch (RuntimeException | Error closeFailure) {
                    failure.addSuppressed(closeFailure);
                }
            }

            if (commandBuffer != null) {
                try {
                    commandBuffer.close();
                } catch (RuntimeException | Error closeFailure) {
                    failure.addSuppressed(closeFailure);
                }
            }

            throw failure;
        }

        this.handle = handle;
        this.commandBuffer = commandBuffer;
        this.scratchArena = scratchArena;
    }

    public void addWire(int wireId) {
        requireUsable();
        commandBuffer.addWire(wireId);
    }

    public void removeWire(int wireId) {
        requireUsable();
        commandBuffer.removeWire(wireId);
    }

    public void connectWires(int wireAId, int wireBId) {
        requireUsable();
        commandBuffer.connectWires(wireAId, wireBId);
    }

    public void disconnectWires(int wireAId, int wireBId) {
        requireUsable();
        commandBuffer.disconnectWires(wireAId, wireBId);
    }

    public void addDevice(int deviceId, int definitionId) {
        requireUsable();
        commandBuffer.addDevice(deviceId, definitionId);
    }

    public void removeDevice(int deviceId) {
        requireUsable();
        commandBuffer.removeDevice(deviceId);
    }

    public void attachTerminal(
            int wireId,
            int deviceId,
            int terminalId
    ) {
        requireUsable();

        commandBuffer.attachTerminal(
                wireId,
                deviceId,
                terminalId
        );
    }

    public void detachTerminal(
            int wireId,
            int deviceId,
            int terminalId
    ) {
        requireUsable();

        commandBuffer.detachTerminal(
                wireId,
                deviceId,
                terminalId
        );
    }

    public void setDeviceParameter(
            int deviceId,
            int parameterId,
            double value
    ) {
        requireUsable();

        commandBuffer.setDeviceParameter(
                deviceId,
                parameterId,
                value
        );
    }

    public void applyCommands() {
        MemorySegment world = requireUsable();

        if (commandBuffer.isEmpty()) {
            return;
        }

        MemorySegment input = commandBuffer.segmentForApply();
        int inputLength = commandBuffer.byteSize();

        try {
            final int code;

            try {
                code = NativeBindings.applyCommands(
                        world,
                        input,
                        inputLength,
                        commandResult
                );
            } catch (RuntimeException | Error failure) {
                poisoned = true;
                throw failure;
            }

            if (code != 0) {
                poisoned = true;

                int commandIndex = commandResult.get(
                        ValueLayout.JAVA_INT,
                        NativeLayouts.COMMAND_RESULT_COMMAND_INDEX_OFFSET
                );

                int byteOffset = commandResult.get(
                        ValueLayout.JAVA_INT,
                        NativeLayouts.COMMAND_RESULT_BYTE_OFFSET
                );

                throw new IllegalStateException(
                        "Native world command application failed: code="
                                + Integer.toUnsignedLong(code)
                                + ", commandIndex="
                                + Integer.toUnsignedLong(commandIndex)
                                + ", byteOffset="
                                + Integer.toUnsignedLong(byteOffset)
                );
            }
        } finally {
            commandBuffer.clear();
        }
    }

    MemorySegment requireOpen() {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalStateException(
                    "Electrical world is closed"
            );
        }

        return handle;
    }

    private MemorySegment requireUsable() {
        MemorySegment world = requireOpen();

        if (poisoned) {
            throw new IllegalStateException(
                    "Electrical world is unusable after a command application failure"
            );
        }

        return world;
    }

    @Override
    public void close() {
        if (MemorySegment.NULL.equals(handle)) {
            return;
        }

        MemorySegment world = handle;

        commandBuffer.close();
        scratchArena.close();

        handle = MemorySegment.NULL;

        NativeBindings.destroyWorld(world);
    }
}