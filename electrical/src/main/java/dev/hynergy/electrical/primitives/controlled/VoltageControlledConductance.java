package dev.hynergy.electrical.primitives.controlled;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledConductance extends Device {
    public static final DeviceType<VoltageControlledConductance> TYPE =
        DeviceType.primitive(10, VoltageControlledConductance::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL = 2;

    private static final int PARAMETER_THRESHOLD_VOLTAGE = 0;
    private static final int PARAMETER_TRANSITION_VOLTAGE = 1;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 2;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 3;

    private VoltageControlledConductance() {
    }

    public static VoltageControlledConductance create(
        ElectricSystem system,
        double thresholdVoltage,
        double transitionVoltage,
        double minimumConductance,
        double maximumConductance
    ) {
        VoltageControlledConductance device = system.create(TYPE);

        device.setThresholdVoltage(thresholdVoltage);
        device.setTransitionVoltage(transitionVoltage);
        device.setMinimumConductance(minimumConductance);
        device.setMaximumConductance(maximumConductance);

        return device;
    }

    public void setThresholdVoltage(double value) {
        setParameter(PARAMETER_THRESHOLD_VOLTAGE, value);
    }

    public void setTransitionVoltage(double value) {
        setParameter(PARAMETER_TRANSITION_VOLTAGE, value);
    }

    public void setMinimumConductance(double value) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, value);
    }

    public void setMaximumConductance(double value) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, value);
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

    public void attachControl(Wire wire) {
        attachTerminal(TERMINAL_CONTROL, wire);
    }

    public void detachControl(Wire wire) {
        detachTerminal(TERMINAL_CONTROL, wire);
    }
}
