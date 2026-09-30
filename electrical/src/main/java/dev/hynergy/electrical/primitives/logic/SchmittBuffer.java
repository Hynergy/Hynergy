package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.*;

/**
 * Models a non-inverting Schmitt buffer with a finite-conductance output stage.
 *
 * <p>The input voltage uses VSS as its reference. The lower switching voltage
 * is the threshold voltage minus half the hysteresis width. The upper switching
 * voltage is the threshold voltage plus half the hysteresis width.</p>
 *
 * <p>The output and input observation voltages use VSS as their reference.</p>
 */
public final class SchmittBuffer extends Device {
    /**
     * The device type for {@code SchmittBuffer}.
     */
    public static final DeviceType<SchmittBuffer> TYPE = DeviceType.primitive(18, SchmittBuffer::new);

    private static final int TERMINAL_OUTPUT = 0;
    private static final int TERMINAL_VDD = 1;
    private static final int TERMINAL_VSS = 2;
    private static final int TERMINAL_INPUT = 3;

    private static final int PARAMETER_THRESHOLD_RELATIVE_TO_VSS = 0;
    private static final int PARAMETER_HYSTERESIS_WIDTH = 1;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 2;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 3;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_INPUT_VOLTAGE = 1;
    private static final int OBSERVER_SUPPLY_CURRENT = 2;

    private SchmittBuffer() {
    }

    /**
     * Creates a Schmitt buffer.
     *
     * @param system the electrical system
     * @param thresholdRelativeToVss the center of the hysteresis interval
     *     relative to VSS, in volts; the value must be finite
     * @param hysteresisWidth the hysteresis width, in volts; the value must be
     *     finite and non-negative
     * @param maximumConductance the active output-stage conductance, in
     *     siemens; the value must be finite and greater than zero
     * @param minimumConductance the inactive output-stage conductance, in
     *     siemens; the value must be finite and non-negative, and less than
     *     {@code maximumConductance}
     *
     * @return the Schmitt buffer
     */
    public static SchmittBuffer create(
        ElectricSystem system,
        double thresholdRelativeToVss,
        double hysteresisWidth,
        double maximumConductance,
        double minimumConductance
    ) {
        SchmittBuffer device = system.create(TYPE);

        device.setThresholdRelativeToVss(thresholdRelativeToVss);
        device.setHysteresisWidth(hysteresisWidth);
        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
    }

    /**
     * Sets the center of the hysteresis interval relative to VSS.
     *
     * @param thresholdRelativeToVss the threshold voltage, in volts; the value
     *     must be finite
     */
    public void setThresholdRelativeToVss(double thresholdRelativeToVss) {
        setParameter(PARAMETER_THRESHOLD_RELATIVE_TO_VSS, thresholdRelativeToVss);
    }

    /**
     * Sets the hysteresis width.
     *
     * @param hysteresisWidth the hysteresis width, in volts; the value must be
     *     finite and non-negative
     */
    public void setHysteresisWidth(double hysteresisWidth) {
        setParameter(PARAMETER_HYSTERESIS_WIDTH, hysteresisWidth);
    }

    /**
     * Sets the maximum output-stage conductance.
     *
     * @param maximumConductance the conductance, in siemens; the value must be
     *     finite, greater than zero, and greater than the current minimum
     *     conductance
     */
    public void setMaximumConductance(double maximumConductance) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, maximumConductance);
    }

    /**
     * Sets the minimum output-stage conductance.
     *
     * @param minimumConductance the conductance, in siemens; the value must be
     *     finite, non-negative, and less than the current maximum conductance
     */
    public void setMinimumConductance(double minimumConductance) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, minimumConductance);
    }

    /**
     * Attaches the output terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachOutput(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT, wire);
    }

    /**
     * Detaches the output terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachOutput(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT, wire);
    }

    /**
     * Attaches the VDD terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachVdd(Wire wire) {
        attachTerminal(TERMINAL_VDD, wire);
    }

    /**
     * Detaches the VDD terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachVdd(Wire wire) {
        detachTerminal(TERMINAL_VDD, wire);
    }

    /**
     * Attaches the VSS terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachVss(Wire wire) {
        attachTerminal(TERMINAL_VSS, wire);
    }

    /**
     * Detaches the VSS terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachVss(Wire wire) {
        detachTerminal(TERMINAL_VSS, wire);
    }

    /**
     * Attaches the input terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachInput(Wire wire) {
        attachTerminal(TERMINAL_INPUT, wire);
    }

    /**
     * Detaches the input terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachInput(Wire wire) {
        detachTerminal(TERMINAL_INPUT, wire);
    }

    /**
     * Subscribes to the output voltage relative to VSS.
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeOutputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_VOLTAGE, listener);
    }

    /**
     * Subscribes to the input voltage relative to VSS.
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeInputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_INPUT_VOLTAGE, listener);
    }

    /**
     * Subscribes to the current from VDD to the output through the pull-up
     * branch.
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeSupplyCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_SUPPLY_CURRENT, listener);
    }
}
