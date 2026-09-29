package dev.hynergy.electrical.primitives.controlled;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledConductance extends Device {
    public static final DeviceType<VoltageControlledConductance> TYPE =
        DeviceType.primitive(10, VoltageControlledConductance::new);


    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;
    private static final int TERMINAL_C = 2;

    private static final int PARAMETER_THRESHOLD_VOLTAGE = 0;
    private static final int PARAMETER_TRANSITION_VOLTAGE = 1;
    private static final int PARAMETER_G_MIN = 2;
    private static final int PARAMETER_G_MAX = 3;


    private VoltageControlledConductance() {
    }

    public static VoltageControlledConductance create(
        ElectricSystem system,
        double vThreshold,
        double vTransition,
        double gMin,
        double gMax
    ) {
        VoltageControlledConductance device = system.create(TYPE);

        device.setVoltageThreshold(vThreshold);
        device.setTransitionVoltage(vTransition);
        device.setGMax(gMax);
        device.setGMin(gMin);

        return device;
    }


    public void setVoltageThreshold(double value) {
        setParameter(PARAMETER_THRESHOLD_VOLTAGE, value);
    }

    public void setTransitionVoltage(double value) {
        setParameter(PARAMETER_TRANSITION_VOLTAGE, value);
    }

    public void setGMax(double value) {
        setParameter(PARAMETER_G_MAX, value);
    }

    public void setGMin(double value) {
        setParameter(PARAMETER_G_MIN, value);
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
}
