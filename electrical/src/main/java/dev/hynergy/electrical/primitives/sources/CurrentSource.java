package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.*;

public final class CurrentSource extends Device {
    public static final DeviceType<CurrentSource> TYPE = DeviceType.primitive(4, CurrentSource::new);


    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_CURRENT = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;


    private CurrentSource() {
    }

    public static CurrentSource create(ElectricSystem system, double current) {
        CurrentSource device = system.create(TYPE);

        device.setCurrent(current);

        return device;
    }


    public void setCurrent(double current) {
        setParameter(PARAMETER_CURRENT, current);
    }


    public void attachPositive(Wire wire) {
        attachTerminal(TERMINAL_POSITIVE, wire);
    }

    public void attachNegative(Wire wire) {
        attachTerminal(TERMINAL_NEGATIVE, wire);
    }

    public void detachPositive(Wire wire) {
        detachTerminal(TERMINAL_POSITIVE, wire);
    }

    public void detachNegative(Wire wire) {
        detachTerminal(TERMINAL_NEGATIVE, wire);
    }


    public ObservationSubscription observeVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_VOLTAGE, listener);
    }

    public ObservationSubscription observeCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_CURRENT, listener);
    }
}
