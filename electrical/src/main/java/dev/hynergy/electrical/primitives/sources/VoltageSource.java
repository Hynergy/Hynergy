package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageSource extends Device {
    public static final DeviceType<VoltageSource> TYPE = DeviceType.primitive(3, VoltageSource::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_VOLTAGE = 0;

    private VoltageSource() {
    }

    public static VoltageSource create(ElectricSystem system, double voltage) {
        VoltageSource device = system.create(TYPE);

        device.setVoltage(voltage);

        return device;
    }

    public void setVoltage(double value) {
        setParameter(PARAMETER_VOLTAGE, value);
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
