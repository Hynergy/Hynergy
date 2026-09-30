package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

public final class Resistance extends Device {
    public static final DeviceType<Resistance> TYPE = DeviceType.primitive(1, Resistance::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_RESISTANCE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;


    private Resistance() {
    }

    public static Resistance create(ElectricSystem system, double resistance) {
        Resistance device = system.create(TYPE);

        device.setResistance(resistance);

        return device;
    }


    public void setResistance(double resistance) {
        setParameter(PARAMETER_RESISTANCE, resistance);
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
