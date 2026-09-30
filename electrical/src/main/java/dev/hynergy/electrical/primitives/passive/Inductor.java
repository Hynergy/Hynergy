package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

public final class Inductor extends Device {
    public static final DeviceType<Inductor> TYPE = DeviceType.primitive(8, Inductor::new);


    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_INDUCTANCE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;


    private Inductor() {
    }

    public static Inductor create(ElectricSystem system, double inductance) {
        Inductor device = system.create(TYPE);

        device.setInductance(inductance);

        return device;
    }


    public void setInductance(double inductance) {
        setParameter(PARAMETER_INDUCTANCE, inductance);
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
