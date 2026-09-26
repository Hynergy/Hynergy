package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;
import dev.hynergy.electrical.internal.NativeLayouts;
import org.jspecify.annotations.NonNull;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

public final class ElectricalWorld implements AutoCloseable {

    private static final class SubscriptionCode {
        static final int SUCCESS = 0;

        static final int NULL_WORLD = 1;
        static final int NULL_RESULT = 2;
        static final int INVALID_DEVICE_ID = 4;
        static final int INVALID_SUBSCRIPTION_ID = 5;

        static final int UNKNOWN_DEVICE = 20;
        static final int UNKNOWN_OBSERVER = 21;
        static final int ID_EXHAUSTED = 22;
        static final int UNKNOWN_SUBSCRIPTION = 23;

        static final int INTERNAL_PANIC = -1;
    }

    private final WorldCommandBuffer commandBuffer;
    private final Arena scratchArena;
    private final MemorySegment commandResult;
    private final MemorySegment subscriptionIdResult;

    private MemorySegment handle;
    private boolean poisoned;

    ElectricalWorld(MemorySegment handle) {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalArgumentException("World handle must not be null");
        }

        WorldCommandBuffer commandBuffer = null;
        Arena scratchArena = null;

        try {
            commandBuffer = new WorldCommandBuffer();
            scratchArena = Arena.ofConfined();

            this.commandResult = scratchArena.allocate(NativeLayouts.COMMAND_RESULT);
            this.subscriptionIdResult = scratchArena.allocate(ValueLayout.JAVA_INT);
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

    public void attachTerminal(int wireId, int deviceId, int terminalId) {
        requireUsable();
        commandBuffer.attachTerminal(wireId, deviceId, terminalId);
    }

    public void detachTerminal(int wireId, int deviceId, int terminalId) {
        requireUsable();
        commandBuffer.detachTerminal(wireId, deviceId, terminalId);
    }

    public void setDeviceParameter(int deviceId, int parameterId, double value) {
        requireUsable();
        commandBuffer.setDeviceParameter(deviceId, parameterId, value);
    }

    public void applyCommands() {
        MemorySegment world = requireUsable();

        if (commandBuffer.isEmpty()) {
            return;
        }

        MemorySegment input = commandBuffer.encodedSegment();
        int inputLength = commandBuffer.byteSize();

        try {
            final int code;

            try {
                code = NativeBindings.applyCommands(world, input, inputLength, commandResult);
            } catch (RuntimeException | Error failure) {
                poisoned = true;
                throw failure;
            }

            if (code != 0) {
                poisoned = true;

                int commandIndex = commandResult.get(ValueLayout.JAVA_INT,
                    NativeLayouts.COMMAND_RESULT_COMMAND_INDEX_OFFSET
                );

                int byteOffset = commandResult.get(ValueLayout.JAVA_INT,
                    NativeLayouts.COMMAND_RESULT_BYTE_OFFSET
                );

                throw new IllegalStateException(
                    "Native world command application failed: code=" + Integer.toUnsignedLong(code)
                        + ", commandIndex=" + Integer.toUnsignedLong(commandIndex) + ", byteOffset="
                        + Integer.toUnsignedLong(byteOffset));
            }
        } finally {
            commandBuffer.clear();
        }
    }

    public int subscribeObserver(int deviceId, int observerId) {
        requireUsable();

        if (deviceId <= 0) {
            throw new IllegalArgumentException("Device ID must be greater than zero");
        }

        if (observerId < 0) {
            throw new IllegalArgumentException("Observer ID must be non-negative");
        }

        applyCommands();

        int code = NativeBindings.subscribeObserver(requireUsable(),
            deviceId,
            observerId,
            subscriptionIdResult
        );

        if (code != SubscriptionCode.SUCCESS) {
            handleSubscriptionFailure("create subscription", code);
        }

        int subscriptionId = subscriptionIdResult.get(ValueLayout.JAVA_INT, 0);

        if (subscriptionId == 0) {
            poisoned = true;

            throw new IllegalStateException(
                "Native subscription creation succeeded with an invalid ID");
        }

        return subscriptionId;
    }

    public void unsubscribe(int subscriptionId) {
        requireUsable();

        if (subscriptionId == 0) {
            throw new IllegalArgumentException("Subscription ID must not be zero");
        }

        applyCommands();

        int code = NativeBindings.unsubscribe(requireUsable(), subscriptionId);

        if (code != SubscriptionCode.SUCCESS) {
            handleSubscriptionFailure("remove subscription", code);
        }
    }

    private void handleSubscriptionFailure(String operation, int code) {
        String reason = switch (code) {
            case SubscriptionCode.NULL_WORLD -> "native world handle is null";

            case SubscriptionCode.NULL_RESULT -> "native subscription result pointer is null";

            case SubscriptionCode.INVALID_DEVICE_ID ->
                "native code rejected the validated device ID";

            case SubscriptionCode.INVALID_SUBSCRIPTION_ID ->
                "native code rejected the validated subscription ID";

            case SubscriptionCode.UNKNOWN_DEVICE -> "device does not exist";

            case SubscriptionCode.UNKNOWN_OBSERVER -> "observer does not exist on the device";

            case SubscriptionCode.ID_EXHAUSTED -> "subscription ID space is exhausted";

            case SubscriptionCode.UNKNOWN_SUBSCRIPTION -> "subscription does not exist";

            case SubscriptionCode.INTERNAL_PANIC -> {
                poisoned = true;
                yield "native engine panicked";
            }

            default -> "unknown native subscription status";
        };

        throw new IllegalStateException(
            "Failed to " + operation + ": " + reason + " (code=" + Integer.toUnsignedLong(code)
                + ")");
    }

    @NonNull MemorySegment requireOpen() {
        if (MemorySegment.NULL.equals(handle)) {
            throw new IllegalStateException("Electrical world is closed");
        }

        return handle;
    }

    private @NonNull MemorySegment requireUsable() {
        MemorySegment world = requireOpen();

        if (poisoned) {
            throw new IllegalStateException(
                "Electrical world is unusable after a command application failure");
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