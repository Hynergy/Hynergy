package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.*;

public final class VoltageControlledVoltageSource extends Device {
    public static final DeviceType<VoltageControlledVoltageSource> TYPE =
        DeviceType.primitive(6, VoltageControlledVoltageSource::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL_POSITIVE = 2;
    private static final int TERMINAL_CONTROL_NEGATIVE = 3;

    private static final int PARAMETER_VOLTAGE_GAIN = 0;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_CONTROL_VOLTAGE = 1;
    private static final int OBSERVER_OUTPUT_CURRENT = 2;


    private VoltageControlledVoltageSource() {
    }

    public static VoltageControlledVoltageSource create(ElectricSystem system, double voltageGain) {
        VoltageControlledVoltageSource device = system.create(TYPE);

        device.setVoltageGain(voltageGain);

        return device;
    }

    public void setVoltageGain(double value) {
        setParameter(PARAMETER_VOLTAGE_GAIN, value);
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


    public ObservationSubscription observeOutputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_VOLTAGE, listener);
    }

    public ObservationSubscription observeControlVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_CONTROL_VOLTAGE, listener);
    }

    public ObservationSubscription observeOutputCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_CURRENT, listener);
    }
}
