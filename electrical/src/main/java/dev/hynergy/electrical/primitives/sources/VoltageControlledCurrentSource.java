package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledCurrentSource extends Device {
    public static final DeviceType<VoltageControlledCurrentSource> TYPE =
        DeviceType.primitive(5, VoltageControlledCurrentSource::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL_POSITIVE = 2;
    private static final int TERMINAL_CONTROL_NEGATIVE = 3;

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

    public void attachOutputPositive(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    public void detachOutputPositive(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    public void attachOutputNegative(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }

    public void detachOutputNegative(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }

    public void attachControlPositive(Wire wire) {
        attachTerminal(TERMINAL_CONTROL_POSITIVE, wire);
    }

    public void detachControlPositive(Wire wire) {
        detachTerminal(TERMINAL_CONTROL_POSITIVE, wire);
    }

    public void attachControlNegative(Wire wire) {
        attachTerminal(TERMINAL_CONTROL_NEGATIVE, wire);
    }

    public void detachControlNegative(Wire wire) {
        detachTerminal(TERMINAL_CONTROL_NEGATIVE, wire);
    }
}
