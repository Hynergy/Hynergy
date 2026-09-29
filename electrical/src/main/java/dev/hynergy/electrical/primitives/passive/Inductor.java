package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Inductor extends Device {
    public static final DeviceType<Inductor> TYPE = DeviceType.primitive(8, Inductor::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_INDUCTANCE = 0;

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
