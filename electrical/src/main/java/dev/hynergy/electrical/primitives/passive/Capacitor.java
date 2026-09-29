package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Capacitor extends Device {
    public static final DeviceType<Capacitor> TYPE = DeviceType.primitive(7, Capacitor::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_CAPACITANCE = 0;

    private Capacitor() {
    }

    public static Capacitor create(ElectricSystem system, double capacitance) {
        Capacitor device = system.create(TYPE);

        device.setCapacitance(capacitance);

        return device;
    }

    public void setCapacitance(double capacitance) {
        setParameter(PARAMETER_CAPACITANCE, capacitance);
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
}
