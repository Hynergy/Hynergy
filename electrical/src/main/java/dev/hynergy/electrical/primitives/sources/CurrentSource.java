package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class CurrentSource extends Device {
    public static final DeviceType<CurrentSource> TYPE = DeviceType.primitive(4, CurrentSource::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_CURRENT = 0;

    private CurrentSource() {
    }

    public static CurrentSource create(ElectricSystem system, double current) {
        CurrentSource device = system.create(TYPE);

        device.setCurrent(current);

        return device;
    }

    public void setCurrent(double value) {
        setParameter(PARAMETER_CURRENT, value);
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
