package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.*;

public final class Not extends Device {
    public static final DeviceType<Not> TYPE = DeviceType.primitive(13, Not::new);

    private static final int TERMINAL_OUTPUT = 0;
    private static final int TERMINAL_VDD = 1;
    private static final int TERMINAL_VSS = 2;
    private static final int TERMINAL_INPUT = 3;

    private static final int PARAMETER_THRESHOLD_RELATIVE_TO_VSS = 0;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 1;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 2;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_INPUT_VOLTAGE = 1;
    private static final int OBSERVER_OUTPUT_CURRENT = 2;

    private Not() {
    }

    public static Not create(
        ElectricSystem system,
        double thresholdRelativeToVss,
        double maximumConductance,
        double minimumConductance
    ) {
        Not device = system.create(TYPE);

        device.setThresholdRelativeToVss(thresholdRelativeToVss);
        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
    }

    public void setThresholdRelativeToVss(double value) {
        setParameter(PARAMETER_THRESHOLD_RELATIVE_TO_VSS, value);
    }

    public void setMaximumConductance(double value) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, value);
    }

    public void setMinimumConductance(double value) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, value);
    }

    public void attachOutput(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT, wire);
    }

    public void detachOutput(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT, wire);
    }

    public void attachVdd(Wire wire) {
        attachTerminal(TERMINAL_VDD, wire);
    }

    public void detachVdd(Wire wire) {
        detachTerminal(TERMINAL_VDD, wire);
    }

    public void attachVss(Wire wire) {
        attachTerminal(TERMINAL_VSS, wire);
    }

    public void detachVss(Wire wire) {
        detachTerminal(TERMINAL_VSS, wire);
    }

    public void attachInput(Wire wire) {
        attachTerminal(TERMINAL_INPUT, wire);
    }

    public void detachInput(Wire wire) {
        detachTerminal(TERMINAL_INPUT, wire);
    }

    public ObservationSubscription observeOutputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_VOLTAGE, listener);
    }

    public ObservationSubscription observeInputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_INPUT_VOLTAGE, listener);
    }

    public ObservationSubscription observeOutputCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_CURRENT, listener);
    }
}
