package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

public final class Diode extends Device {
    public static final DeviceType<Diode> TYPE = DeviceType.primitive(12, Diode::new);


    private static final int TERMINAL_ANODE = 0;
    private static final int TERMINAL_CATHODE = 1;

    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 0;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 1;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    
    private Diode() {
    }

    public static Diode create(ElectricSystem system, double maximumConductance, double minimumConductance) {
        Diode device = system.create(TYPE);

        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
    }


    public void setMaximumConductance(double value) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, value);
    }

    public void setMinimumConductance(double value) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, value);
    }


    public void attachAnode(Wire wire) {
        attachTerminal(TERMINAL_ANODE, wire);
    }

    public void attachCathode(Wire wire) {
        attachTerminal(TERMINAL_CATHODE, wire);
    }

    public void detachAnode(Wire wire) {
        detachTerminal(TERMINAL_ANODE, wire);
    }

    public void detachCathode(Wire wire) {
        detachTerminal(TERMINAL_CATHODE, wire);
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
