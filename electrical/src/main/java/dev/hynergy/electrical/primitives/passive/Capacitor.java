package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Capacitor extends Device {
    public static final DeviceType<Capacitor> TYPE = DeviceType.primitive(7, Capacitor::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_CAPACITANCE = 0;

    private Capacitor() {
    }

    public static Capacitor create(ElectricSystem system, double capacitance) {
        Capacitor device = system.create(TYPE);

        device.setCapacitance(capacitance);

        return device;
    }

    public void setCapacitance(double value) {
        setParameter(PARAMETER_CAPACITANCE, value);
    }

    public void attachA(Wire wire) {
        attachTerminal(TERMINAL_A, wire);
    }

    public void attachB(Wire wire) {
        attachTerminal(TERMINAL_B, wire);
    }

    public void detachA(Wire wire) {
        detachTerminal(TERMINAL_A, wire);
    }

    public void detachB(Wire wire) {
        detachTerminal(TERMINAL_B, wire);
    }
}
