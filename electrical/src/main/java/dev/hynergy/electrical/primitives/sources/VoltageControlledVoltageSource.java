package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledVoltageSource extends Device {
    public static final DeviceType<VoltageControlledVoltageSource> TYPE =
        DeviceType.primitive(6, VoltageControlledVoltageSource::new);


    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;
    private static final int TERMINAL_C = 2;
    private static final int TERMINAL_D = 3;

    private static final int PARAMETER_GAIN = 0;


    private VoltageControlledVoltageSource() {
    }

    public static VoltageControlledVoltageSource create(ElectricSystem system, double gain) {
        VoltageControlledVoltageSource device = system.create(TYPE);

        device.setGain(gain);

        return device;
    }


    public void setGain(double value) {
        setParameter(PARAMETER_GAIN, value);
    }


    public void attachA(Wire wire) {
        attachTerminal(TERMINAL_A, wire);
    }

    public void detachA(Wire wire) {
        detachTerminal(TERMINAL_A, wire);
    }

    public void attachB(Wire wire) {
        attachTerminal(TERMINAL_B, wire);
    }

    public void detachB(Wire wire) {
        detachTerminal(TERMINAL_B, wire);
    }

    public void attachC(Wire wire) {
        attachTerminal(TERMINAL_C, wire);
    }

    public void detachC(Wire wire) {
        detachTerminal(TERMINAL_C, wire);
    }

    public void attachD(Wire wire) {
        attachTerminal(TERMINAL_D, wire);
    }

    public void detachD(Wire wire) {
        detachTerminal(TERMINAL_D, wire);
    }
}
