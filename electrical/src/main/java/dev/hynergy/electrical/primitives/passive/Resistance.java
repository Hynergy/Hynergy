package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Resistance extends Device {
    public static final DeviceType<Resistance> TYPE = DeviceType.primitive(1, Resistance::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_RESISTANCE = 0;

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
