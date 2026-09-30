package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;

/**
 * Controls one electrical observation subscription.
 *
 * <p>A subscription stays active until it is unsubscribed, until its
 * device is removed, or until its electrical system closes.</p>
 *
 * <p>An inactive subscription cannot receive a record from a future
 * tick. A completed tick can still contain a record for a subscription
 * that becomes inactive during publication.</p>
 */
public final class ObservationSubscription {
    private final int nativeId;
    private final int deviceId;

    private @Nullable ElectricalSystem system;
    private @Nullable ObservationListener listener;

    ObservationSubscription(ElectricalSystem system, int nativeId, int deviceId, ObservationListener listener) {
        if (nativeId == 0) {
            throw new IllegalArgumentException("Native subscription ID must not be zero");
        }

        if (deviceId <= 0) {
            throw new IllegalArgumentException("Device ID must be positive");
        }

        this.system = Objects.requireNonNull(system, "system");
        this.nativeId = nativeId;
        this.deviceId = deviceId;
        this.listener = Objects.requireNonNull(listener, "listener");
    }

    /**
     * Stops this observation subscription.
     *
     * <p>If the subscription is inactive, this method has no effect.</p>
     *
     * <p>If this method is called during observation publication, the
     * completed tick can still contain a record for this subscription.</p>
     */
    public void unsubscribe() {
        ElectricalSystem system = this.system;

        if (system != null) {
            system.unsubscribe(this);
        }
    }

    int nativeId() {
        return nativeId;
    }

    int deviceId() {
        return deviceId;
    }

    boolean belongsTo(ElectricalSystem system) {
        return this.system == system;
    }

    ObservationListener listener() {
        ObservationListener listener = this.listener;

        if (listener == null) {
            throw new IllegalStateException("Observation subscription listener has been released");
        }

        return listener;
    }

    void deactivate() {
        system = null;
    }

    void releaseListener() {
        listener = null;
    }


    /**
     * Tests whether this subscription can receive records from a future tick.
     *
     * @return {@code true} if the subscription is active
     */
    public boolean isActive() {
        return system != null;
    }
}
