package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;

public final class ElectricSystem implements AutoCloseable {
    private final ElectricalRuntime runtime;
    private final ElectricalWorld world;

    private final ObservationSubscriptionRegistry subscriptions = new ObservationSubscriptionRegistry();

    private boolean closed;
    private boolean poisoned;
    private boolean ticking;

    ElectricSystem(ElectricalRuntime runtime, ElectricalWorld world) {
        this.runtime = runtime;
        this.world = world;
    }


    public void tick() {
        requireUsable();

        if (ticking) {
            throw new IllegalStateException("Recursive calls to tick are not allowed");
        }

        ticking = true;

        try {
            int recordCount = world.tick();

            if (recordCount == 0) {
                return;
            }

            Throwable callbackFailure = null;
            Throwable publicationFailure = null;
            boolean publishing = false;

            try {
                subscriptions.beginPublication();
                publishing = true;

                for (int index = 0; index < recordCount; index++) {
                    int subscriptionId = world.subscriptionIdAt(index);
                    int statusCode = world.subscriptionStatusAt(index);
                    double value = world.subscriptionValueAt(index);

                    ObservationStatus status = observationStatus(statusCode);

                    ObservationSubscription subscription = subscriptions.get(subscriptionId);

                    if (subscription == null) {
                        throw new IllegalStateException(
                            "Native tick returned unknown subscription ID: " + Integer.toUnsignedLong(subscriptionId));
                    }

                    try {
                        subscription.listener().onUpdate(status, value);
                    } catch (RuntimeException | Error failure) {
                        callbackFailure = appendFailure(callbackFailure, failure);
                    }
                }
            } catch (RuntimeException | Error failure) {
                poisoned = true;
                publicationFailure = failure;
            } finally {
                if (publishing) {
                    try {
                        subscriptions.endPublication();
                    } catch (RuntimeException | Error failure) {
                        poisoned = true;
                        publicationFailure = appendFailure(publicationFailure, failure);
                    }
                }
            }

            if (publicationFailure != null) {
                if (callbackFailure != null) {
                    publicationFailure.addSuppressed(callbackFailure);
                }

                rethrow(publicationFailure);
            }

            if (callbackFailure != null) {
                rethrow(callbackFailure);
            }
        } finally {
            ticking = false;
        }
    }

    private static ObservationStatus observationStatus(int statusCode) {
        return switch (statusCode) {
            case ElectricalWorld.SubscriptionStatusCode.AVAILABLE -> ObservationStatus.AVAILABLE;
            case ElectricalWorld.SubscriptionStatusCode.UNAVAILABLE -> ObservationStatus.UNAVAILABLE;
            
            default -> throw new IllegalStateException(
                "Unknown native observation status: " + Integer.toUnsignedLong(statusCode));
        };
    }

    public Wire createWire() {
        requireUsable();

        int id = world.addWire();
        int generation = world.wireGeneration(id);

        return new Wire(this, id, generation);
    }

    void connect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.connectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void disconnect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.disconnectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void remove(Wire wire) {
        requireOwned(wire);

        world.removeWire(wire.id(), wire.generation());
    }

    public <T extends Device> T create(DeviceType<T> type) {
        requireUsable();

        Objects.requireNonNull(type, "type");

        DeviceDefinition definition = runtime.requireDefinition(type);

        T device = type.construct();

        device.requireUnbound();

        int id = world.addDevice(definition);
        int generation = world.deviceGeneration(id);

        device.bind(this, id, generation);

        return device;
    }

    void setParameter(Device device, int parameterId, double value) {
        requireOwned(device);

        world.setDeviceParameter(device.id(), device.generation(), parameterId, value);
    }

    void attachTerminal(Device device, int terminalId, Wire wire) {
        requireOwned(device);
        requireOwned(wire);

        world.attachTerminal(wire.id(), wire.generation(), device.id(), device.generation(), terminalId);
    }

    void detachTerminal(Device device, int terminalId, Wire wire) {
        requireOwned(device);
        requireOwned(wire);

        world.detachTerminal(wire.id(), wire.generation(), device.id(), device.generation(), terminalId);
    }

    ObservationSubscription subscribe(Device device, int observerId, ObservationListener listener) {
        requireOwned(device);

        Objects.requireNonNull(listener, "listener");

        int deviceId = device.id();
        int deviceGeneration = device.generation();

        int subscriptionId = world.subscribeObserver(deviceId, deviceGeneration, observerId);

        try {
            ObservationSubscription subscription =
                new ObservationSubscription(this, subscriptionId, deviceId, listener);

            subscriptions.add(subscription);

            return subscription;
        } catch (RuntimeException | Error failure) {
            try {
                world.unsubscribe(subscriptionId);
            } catch (RuntimeException | Error rollbackFailure) {
                poisoned = true;
                failure.addSuppressed(rollbackFailure);
            }

            throw failure;
        }
    }

    void unsubscribe(
        ObservationSubscription subscription
    ) {
        requireUsable();

        Objects.requireNonNull(subscription, "subscription");

        if (!subscription.belongsTo(this)) {
            throw new IllegalArgumentException("Observation subscription does not belong to this electrical system");
        }

        try {
            world.unsubscribe(subscription.nativeId());
        } catch (ElectricalWorld.SubscriptionOperationException failure) {
            if (failure.isOwnershipConsistencyFailure()) {
                poisoned = true;
            }

            throw failure;
        }

        try {
            subscriptions.remove(subscription);
        } catch (RuntimeException | Error failure) {

            poisoned = true;
            throw failure;
        }
    }

    void remove(Device device) {
        requireOwned(device);

        int deviceId = device.id();

        world.removeDevice(deviceId, device.generation());

        try {
            subscriptions.invalidateDevice(deviceId);
        } catch (RuntimeException | Error failure) {
            poisoned = true;
            throw failure;
        }
    }

    private void requireOwned(Wire wire) {
        requireUsable();

        if (!wire.belongsTo(this)) {
            throw new IllegalArgumentException("Wire belongs to another electrical system");
        }
    }

    private void requireOwned(Device device) {
        requireUsable();

        if (!device.belongsTo(this)) {
            throw new IllegalArgumentException("Device belongs to another electrical system");
        }
    }

    private void requireUsable() {
        if (closed) {
            throw new IllegalStateException("Electrical system is closed");
        }

        if (poisoned) {
            throw new IllegalStateException("Electrical system is unusable after an unrecoverable failure");
        }
    }

    private static Throwable appendFailure(@Nullable Throwable existing, Throwable additional) {
        if (existing == null) {
            return additional;
        }

        existing.addSuppressed(additional);
        return existing;
    }

    private static void rethrow(Throwable failure) {
        if (failure instanceof RuntimeException exception) {
            throw exception;
        }

        if (failure instanceof Error error) {
            throw error;
        }

        throw new AssertionError(failure);
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }

        if (ticking) {
            throw new IllegalStateException("Electrical system cannot close while a tick is in progress");
        }

        Throwable failure = null;

        try {
            world.close();
        } catch (RuntimeException | Error closeFailure) {

            if (world.isOpen()) {
                poisoned = true;
                throw closeFailure;
            }

            failure = closeFailure;
        }

        if (world.isOpen()) {
            poisoned = true;

            throw new IllegalStateException("Electrical world remained open after close completed");
        }

        try {
            subscriptions.invalidateAll();
        } catch (RuntimeException | Error cleanupFailure) {
            if (failure == null) {
                failure = cleanupFailure;
            } else {
                failure.addSuppressed(cleanupFailure);
            }
        }

        closed = true;

        try {
            runtime.releaseSystem();
        } catch (RuntimeException | Error releaseFailure) {
            if (failure == null) {
                failure = releaseFailure;
            } else {
                failure.addSuppressed(releaseFailure);
            }
        }

        if (failure != null) {
            rethrow(failure);
        }
    }
}