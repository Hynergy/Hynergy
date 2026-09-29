package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Conductance extends Device {
    public static final DeviceType<Conductance> TYPE = DeviceType.primitive(2, Conductance::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_CONDUCTANCE = 0;

    private Conductance() {
    }

    public static Conductance create(ElectricSystem system, double conductance) {
        Conductance device = system.create(TYPE);

        device.setConductance(conductance);

        return device;
    }

    public void setConductance(double conductance) {
        setParameter(PARAMETER_CONDUCTANCE, conductance);
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
