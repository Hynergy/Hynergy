package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledCurrentSource extends Device {
    public static final DeviceType<VoltageControlledCurrentSource> TYPE =
        DeviceType.primitive(5, VoltageControlledCurrentSource::new);


    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;
    private static final int TERMINAL_C = 2;
    private static final int TERMINAL_D = 3;

    private static final int PARAMETER_TRANSCONDUCTANCE = 0;


    private VoltageControlledCurrentSource() {
    }

    public static VoltageControlledCurrentSource create(ElectricSystem system, double transconductance) {
        VoltageControlledCurrentSource device = system.create(TYPE);

        device.setTransconductance(transconductance);

        return device;
    }


    public void setTransconductance(double value) {
        setParameter(PARAMETER_TRANSCONDUCTANCE, value);
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