package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Conductance extends Device {
    public static final DeviceType<Conductance> TYPE = DeviceType.primitive(2, Conductance::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_CONDUCTANCE = 0;

    private Conductance() {
    }

    public static Conductance create(ElectricSystem system, double conductance) {
        Conductance device = system.create(TYPE);

        device.setConductance(conductance);

        return device;
    }

    public void setConductance(double value) {
        setParameter(PARAMETER_CONDUCTANCE, value);
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
