package dev.hynergy.electrical;

import dev.hynergy.electrical.internal.NativeBindings;
import dev.hynergy.electrical.internal.NativeLayouts;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

final class ElectricalWorld implements AutoCloseable {

    private final Arena arena;
    private final MemorySegment handle;

    private final WorldIdAllocator wireIds;
    private final WorldIdAllocator deviceIds;

    private final WorldCommandBuffer commandBuffer;
    private final SubscriptionRecordBuffer subscriptionBuffer;

    private final MemorySegment commandResult;
    private final MemorySegment subscriptionIdResult;
    private final MemorySegment tickResult;


    private boolean poisoned;

    ElectricalWorld(Arena arena, MemorySegment handle) {
        this.arena = arena;
        this.handle = handle;

        WorldCommandBuffer commandBuffer = null;
        SubscriptionRecordBuffer subscriptionBuffer = null;

        try {
            this.wireIds = new WorldIdAllocator();
            this.deviceIds = new WorldIdAllocator();

            commandBuffer = new WorldCommandBuffer();
            subscriptionBuffer = new SubscriptionRecordBuffer();

            this.commandResult = arena.allocate(NativeLayouts.COMMAND_RESULT);
            this.subscriptionIdResult = arena.allocate(ValueLayout.JAVA_INT);
            this.tickResult = arena.allocate(NativeLayouts.TICK_RESULT);
        } catch (RuntimeException | Error failure) {
            if (commandBuffer != null) {
                try {
                    commandBuffer.close();
                } catch (RuntimeException | Error closeFailure) {
                    failure.addSuppressed(closeFailure);
                }
            }

            if (subscriptionBuffer != null) {
                try {
                    subscriptionBuffer.close();
                } catch (RuntimeException | Error closeFailure) {
                    failure.addSuppressed(closeFailure);
                }
            }

            throw failure;
        }

        this.commandBuffer = commandBuffer;
        this.subscriptionBuffer = subscriptionBuffer;
    }

    int tick() {
        applyCommands();

        MemorySegment world = requireUsable();
        boolean resized = false;

        while (true) {
            final int code;

            try {
                code = NativeBindings.tickWorld(
                        world,
                        subscriptionBuffer.segment(),
                        subscriptionBuffer.capacity(),
                        tickResult
                );
            } catch (RuntimeException | Error failure) {
                poisoned = true;
                throw failure;
            }

            if (code == TickCode.SUCCESS) {
                return readSuccessfulTickResult();
            }

            if (code == TickCode.BUFFER_TOO_SMALL) {
                if (resized) {
                    poisoned = true;

                    throw new IllegalStateException(
                            "Native world still requires a larger subscription buffer after resize");
                }

                int requiredCapacity =
                        tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_REQUIRED_CAPACITY_OFFSET);

                if (requiredCapacity < 0) {
                    throw new IllegalStateException(
                            "Native subscription count exceeds the supported Java capacity: " + Integer.toUnsignedLong(
                                    requiredCapacity));
                }

                if (requiredCapacity <= subscriptionBuffer.capacity()) {
                    poisoned = true;

                    throw new IllegalStateException(
                            "Native world reported an invalid required subscription capacity: " + requiredCapacity);
                }

                subscriptionBuffer.ensureCapacity(requiredCapacity);
                resized = true;

                continue;
            }

            poisoned = true;

            throw tickFailure(code);
        }
    }

    private int readSuccessfulTickResult() {
        int recordCount = tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_RECORD_COUNT_OFFSET);

        int requiredCapacity = tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_REQUIRED_CAPACITY_OFFSET);

        if (recordCount < 0 || requiredCapacity < 0 || recordCount > requiredCapacity
                || requiredCapacity > subscriptionBuffer.capacity()) {
            poisoned = true;

            throw new IllegalStateException("Native world returned invalid subscription record counts");
        }

        return recordCount;
    }

    private IllegalStateException tickFailure(int code) {
        String reason = switch (code) {
            case TickCode.NULL_WORLD -> "native world handle is null";

            case TickCode.NULL_RESULT -> "native tick result pointer is null";

            case TickCode.NULL_OUTPUT -> "native subscription output pointer is null";

            case TickCode.MISSING_PARAMETER -> {
                int deviceId = tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_DEVICE_ID_OFFSET);

                int parameterId = tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_PARAMETER_ID_OFFSET);

                yield "device " + Integer.toUnsignedLong(deviceId) + " is missing parameter " + Integer.toUnsignedLong(
                        parameterId);
            }

            case TickCode.SINGULAR -> "native solver reported a singular system";

            case TickCode.NONLINEAR_DID_NOT_CONVERGE -> {
                int iterations = tickResult.get(ValueLayout.JAVA_INT, NativeLayouts.TICK_RESULT_ITERATIONS_OFFSET);

                yield "native nonlinear solver did not converge after " + Integer.toUnsignedLong(iterations)
                        + " iterations";
            }

            case TickCode.NON_FINITE_MATRIX -> "native solver produced a non-finite matrix";

            case TickCode.NON_FINITE_SOLUTION -> "native solver produced a non-finite solution";

            case TickCode.RESOURCE_EXHAUSTED -> "native simulation resource limit was exceeded";

            case TickCode.BACKEND_FAILURE -> "native solver backend failed";

            case TickCode.COMPILATION_FAILED -> "native island compilation failed";

            case TickCode.INTERNAL_INVARIANT -> "native simulation invariant failed";

            case TickCode.INTERNAL_PANIC -> "native engine panicked";

            default -> "unknown native tick status";
        };

        return new IllegalStateException(
                "Failed to tick electrical world: " + reason + " (code=" + Integer.toUnsignedLong(code) + ")");
    }


    int addWire() {
        requireUsable();

        int id = wireIds.reserve();
        int generation = wireIds.generation(id);

        try {
            commandBuffer.addWire(id);
        } catch (RuntimeException | Error failure) {
            cancelPendingAdd(wireIds, id, generation, failure);

            throw failure;
        }

        return id;
    }

    int addDevice(DeviceDefinition definition) {
        requireUsable();

        if (definition.id() == 0) {
            throw new IllegalArgumentException("Definition ID must not be zero");
        }

        int id = deviceIds.reserve();
        int generation = deviceIds.generation(id);

        try {
            commandBuffer.addDevice(id, definition.id());
        } catch (RuntimeException | Error failure) {
            cancelPendingAdd(deviceIds, id, generation, failure);

            throw failure;
        }

        return id;
    }

    int wireGeneration(int wireId) {
        requireUsable();
        return wireIds.generation(wireId);
    }

    int deviceGeneration(int deviceId) {
        requireUsable();
        return deviceIds.generation(deviceId);
    }

    void removeWire(WireId wireId) {
        requireUsable();

        wireIds.remove(wireId.value(), wireId.generation());

        try {
            commandBuffer.removeWire(wireId.value());
        } catch (RuntimeException | Error failure) {
            cancelPendingRemove(wireIds, wireId.value(), wireId.generation(), failure);

            throw failure;
        }
    }

    void removeDevice(int deviceId, int generation) {
        requireUsable();

        deviceIds.remove(deviceId, generation);

        try {
            commandBuffer.removeDevice(deviceId);
        } catch (RuntimeException | Error failure) {
            cancelPendingRemove(deviceIds, deviceId, generation, failure);

            throw failure;
        }
    }

    void connectWires(WireId wireAId, WireId wireBId) {
        requireUsable();

        wireIds.requireUsable(wireAId.value(), wireAId.generation());
        wireIds.requireUsable(wireBId.value(), wireBId.generation());

        if (wireAId == wireBId) {
            throw new IllegalArgumentException("A wire cannot be connected to itself");
        }

        commandBuffer.connectWires(wireAId.value(), wireBId.value());
    }

    void disconnectWires(WireId wireAId, WireId wireBId) {
        requireUsable();

        wireIds.requireUsable(wireAId.value(), wireAId.generation());
        wireIds.requireUsable(wireBId.value(), wireBId.generation());

        if (wireAId == wireBId) {
            throw new IllegalArgumentException("A wire cannot be disconnected from itself");
        }

        commandBuffer.disconnectWires(wireAId.value(), wireBId.value());
    }

    void attachTerminal(WireId wireId, int deviceId, int deviceGeneration, int terminalId) {
        requireUsable();

        wireIds.requireUsable(wireId.value(), wireId.generation());

        deviceIds.requireUsable(deviceId, deviceGeneration);

        commandBuffer.attachTerminal(wireId.value(), deviceId, terminalId);
    }

    void detachTerminal(WireId wireId, int deviceId, int deviceGeneration, int terminalId) {
        requireUsable();

        wireIds.requireUsable(wireId.value(), wireId.generation());

        deviceIds.requireUsable(deviceId, deviceGeneration);

        commandBuffer.detachTerminal(wireId.value(), deviceId, terminalId);
    }

    void setDeviceParameter(int deviceId, int deviceGeneration, int parameterId, double value) {
        requireUsable();

        deviceIds.requireUsable(deviceId, deviceGeneration);

        commandBuffer.setDeviceParameter(deviceId, parameterId, value);
    }

    void applyCommands() {
        MemorySegment world = requireUsable();

        if (commandBuffer.isEmpty()) {
            return;
        }

        wireIds.prepareCommitBatch();
        deviceIds.prepareCommitBatch();

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

                int commandIndex =
                        commandResult.get(ValueLayout.JAVA_INT, NativeLayouts.COMMAND_RESULT_COMMAND_INDEX_OFFSET);

                int byteOffset = commandResult.get(ValueLayout.JAVA_INT, NativeLayouts.COMMAND_RESULT_BYTE_OFFSET);

                throw new IllegalStateException(
                        "Native world command application failed: code=" + Integer.toUnsignedLong(code) + ", commandIndex="
                                + Integer.toUnsignedLong(commandIndex) + ", byteOffset=" + Integer.toUnsignedLong(byteOffset));
            }

            try {
                wireIds.commitBatch();
                deviceIds.commitBatch();
            } catch (RuntimeException | Error failure) {
                poisoned = true;

                throw new IllegalStateException(
                        "Native commands were applied, but Java ID state could " + "not be committed",
                        failure
                );
            }
        } finally {
            commandBuffer.clear();
        }
    }

    int subscribeObserver(int deviceId, int deviceGeneration, int observerId) {
        requireUsable();

        deviceIds.requireUsable(deviceId, deviceGeneration);

        if (observerId < 0) {
            throw new IllegalArgumentException("Observer ID must be non-negative");
        }

        applyCommands();

        final int code;

        try {
            code = NativeBindings.subscribeObserver(requireUsable(), deviceId, observerId, subscriptionIdResult);
        } catch (RuntimeException | Error failure) {
            poisoned = true;
            throw failure;
        }

        if (code != SubscriptionCode.SUCCESS) {
            handleSubscriptionFailure("create subscription", code);
        }

        int subscriptionId = subscriptionIdResult.get(ValueLayout.JAVA_INT, 0);

        if (subscriptionId == 0) {
            poisoned = true;

            throw new IllegalStateException("Native subscription creation succeeded with an invalid ID");
        }

        return subscriptionId;
    }

    void unsubscribe(int subscriptionId) {
        requireUsable();

        if (subscriptionId == 0) {
            throw new IllegalArgumentException("Subscription ID must not be zero");
        }

        applyCommands();

        final int code;

        try {
            code = NativeBindings.unsubscribe(requireUsable(), subscriptionId);
        } catch (RuntimeException | Error failure) {
            poisoned = true;
            throw failure;
        }

        if (code != SubscriptionCode.SUCCESS) {
            handleSubscriptionFailure("destroy subscription", code);
        }
    }

    int subscriptionIdAt(int index) {
        return subscriptionBuffer.subscriptionIdAt(index);
    }

    int subscriptionStatusAt(int index) {
        return subscriptionBuffer.subscriptionStatusAt(index);
    }

    double subscriptionValueAt(int index) {
        return subscriptionBuffer.subscriptionValueAt(index);
    }

    private void cancelPendingAdd(WorldIdAllocator allocator, int id, int generation, Throwable failure) {
        try {
            allocator.cancelPendingAdd(id, generation);
        } catch (RuntimeException | Error rollbackFailure) {
            poisoned = true;
            failure.addSuppressed(rollbackFailure);
        }
    }

    private void cancelPendingRemove(WorldIdAllocator allocator, int id, int generation, Throwable failure) {
        try {
            allocator.cancelPendingRemove(id, generation);
        } catch (RuntimeException | Error rollbackFailure) {
            poisoned = true;
            failure.addSuppressed(rollbackFailure);
        }
    }

    private void handleSubscriptionFailure(String operation, int code) {
        boolean ownershipConsistencyFailure =
                code == SubscriptionCode.INVALID_SUBSCRIPTION_ID || code == SubscriptionCode.UNKNOWN_SUBSCRIPTION;

        String reason = switch (code) {
            case SubscriptionCode.NULL_WORLD -> "native world handle is null";

            case SubscriptionCode.NULL_RESULT -> "native subscription result pointer is null";

            case SubscriptionCode.INVALID_DEVICE_ID -> "native code rejected the validated device ID";

            case SubscriptionCode.INVALID_SUBSCRIPTION_ID -> "native code rejected the validated subscription ID";

            case SubscriptionCode.UNKNOWN_DEVICE -> "device does not exist";

            case SubscriptionCode.UNKNOWN_OBSERVER -> "observer does not exist on the device";

            case SubscriptionCode.ID_EXHAUSTED -> "subscription ID space is exhausted";

            case SubscriptionCode.UNKNOWN_SUBSCRIPTION -> "subscription does not exist";

            case SubscriptionCode.INTERNAL_PANIC -> {
                poisoned = true;
                yield "native engine panicked";
            }

            default -> {
                poisoned = true;
                yield "unknown native subscription status";
            }
        };

        throw new SubscriptionOperationException(
                "Failed to " + operation + ": " + reason + " (code=" + Integer.toUnsignedLong(code) + ")",
                ownershipConsistencyFailure
        );
    }

    static final class SubscriptionOperationException extends IllegalStateException {

        private final boolean ownershipConsistencyFailure;

        private SubscriptionOperationException(String message, boolean ownershipConsistencyFailure) {
            super(message);

            this.ownershipConsistencyFailure = ownershipConsistencyFailure;
        }

        boolean isOwnershipConsistencyFailure() {
            return ownershipConsistencyFailure;
        }
    }


    boolean isOpen() {
        return arena.scope().isAlive();
    }

    MemorySegment requireOpen() {
        if (!isOpen()) {
            throw new IllegalStateException("Electrical world is closed");
        }

        return handle;
    }

    private MemorySegment requireUsable() {
        MemorySegment world = requireOpen();

        if (poisoned) {
            throw new IllegalStateException("Electrical world is unusable after an unrecoverable failure");
        }

        return world;
    }

    @Override
    public void close() {
        if (!isOpen()) {
            return;
        }

        RuntimeException failure = null;

        try {
            commandBuffer.close();
        } catch (RuntimeException exception) {
            failure = exception;
        }

        try {
            subscriptionBuffer.close();
        } catch (RuntimeException exception) {
            if (failure == null) {
                failure = exception;
            } else {
                failure.addSuppressed(exception);
            }
        }

        try {
            arena.close();
        } catch (RuntimeException exception) {
            if (failure == null) {
                failure = exception;
            } else {
                failure.addSuppressed(exception);
            }
        }

        if (failure != null) {
            throw failure;
        }
    }


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

    private static final class TickCode {
        static final int SUCCESS = 0;

        static final int NULL_WORLD = 1;
        static final int NULL_RESULT = 2;
        static final int NULL_OUTPUT = 3;
        static final int BUFFER_TOO_SMALL = 5;

        static final int MISSING_PARAMETER = 20;
        static final int SINGULAR = 21;
        static final int NONLINEAR_DID_NOT_CONVERGE = 22;
        static final int NON_FINITE_MATRIX = 23;
        static final int NON_FINITE_SOLUTION = 24;
        static final int RESOURCE_EXHAUSTED = 25;
        static final int BACKEND_FAILURE = 26;
        static final int COMPILATION_FAILED = 27;
        static final int INTERNAL_INVARIANT = 28;

        static final int INTERNAL_PANIC = -1;
    }

    static final class SubscriptionStatusCode {
        static final int AVAILABLE = 0;
        static final int UNAVAILABLE = 1;

        private SubscriptionStatusCode() {
        }
    }
}