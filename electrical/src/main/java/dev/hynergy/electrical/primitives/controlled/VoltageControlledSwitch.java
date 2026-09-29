package dev.hynergy.electrical.primitives.controlled;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class VoltageControlledSwitch extends Device {
    public static final DeviceType<VoltageControlledSwitch> TYPE =
        DeviceType.primitive(9, VoltageControlledSwitch::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL_POSITIVE = 2;
    private static final int TERMINAL_CONTROL_NEGATIVE = 3;

    private static final int PARAMETER_THRESHOLD_VOLTAGE = 0;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 1;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 2;

    private VoltageControlledSwitch() {
    }

    public static VoltageControlledSwitch create(
        ElectricSystem system,
        double thresholdVoltage,
        double maximumConductance,
        double minimumConductance
    ) {
        VoltageControlledSwitch device = system.create(TYPE);

        device.setThresholdVoltage(thresholdVoltage);
        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
    }

    public void setThresholdVoltage(double value) {
        setParameter(PARAMETER_THRESHOLD_VOLTAGE, value);
    }

    public void setMaximumConductance(double value) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, value);
    }

    public void setMinimumConductance(double value) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, value);
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
