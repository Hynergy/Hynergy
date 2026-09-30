package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;

public final class ObservationSubscription {
    private final int nativeId;
    private final int deviceId;

    private @Nullable ElectricSystem system;
    private @Nullable ObservationListener listener;

    ObservationSubscription(ElectricSystem system, int nativeId, int deviceId, ObservationListener listener) {
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

    public boolean isActive() {
        return system != null;
    }

    public void unsubscribe() {
        ElectricSystem system = this.system;

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

    boolean belongsTo(ElectricSystem system) {
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
}
