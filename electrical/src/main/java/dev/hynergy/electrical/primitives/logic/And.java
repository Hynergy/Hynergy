package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.*;

/**
 * Models a two-input AND gate with a finite-conductance output stage.
 *
 * <p>An input is high when its voltage relative to VSS is greater than or
 * equal to the threshold voltage. The output stage uses the maximum and
 * minimum conductances to connect the output towards VDD or VSS.</p>
 *
 * <p>The output and input observation voltages use VSS as their reference.</p>
 */
public final class And extends Device {
    /**
     * The device type for {@code And}.
     */
    public static final DeviceType<And> TYPE = DeviceType.primitive(14, And::new);

    private static final int TERMINAL_OUTPUT = 0;
    private static final int TERMINAL_VDD = 1;
    private static final int TERMINAL_VSS = 2;
    private static final int TERMINAL_INPUT_A = 3;
    private static final int TERMINAL_INPUT_B = 4;

    private static final int PARAMETER_THRESHOLD_RELATIVE_TO_VSS = 0;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 1;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 2;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_INPUT_A_VOLTAGE = 1;
    private static final int OBSERVER_INPUT_B_VOLTAGE = 2;
    private static final int OBSERVER_SUPPLY_CURRENT = 3;

    private And() {
    }

    /**
     * Creates a AND gate.
     *
     * @param system the electrical system
     * @param thresholdRelativeToVss the input threshold relative to VSS, in
     *     volts; the value must be finite
     * @param maximumConductance the active output-stage conductance, in
     *     siemens; the value must be finite and greater than zero
     * @param minimumConductance the inactive output-stage conductance, in
     *     siemens; the value must be finite and non-negative, and less than
     *     {@code maximumConductance}
     *
     * @return the gate
     */
    public static And create(
        ElectricalSystem system,
        double thresholdRelativeToVss,
        double maximumConductance,
        double minimumConductance
    ) {
        And device = system.create(TYPE);

        device.setThresholdRelativeToVss(thresholdRelativeToVss);
        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
    }

    /**
     * Sets the input threshold relative to VSS.
     *
     * @param thresholdRelativeToVss the threshold voltage, in volts; the value
     *     must be finite
     */
    public void setThresholdRelativeToVss(double thresholdRelativeToVss) {
        setParameter(PARAMETER_THRESHOLD_RELATIVE_TO_VSS, thresholdRelativeToVss);
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
     * Attaches input A to a wire.
     *
     * @param wire the wire
     */
    public void attachInputA(Wire wire) {
        attachTerminal(TERMINAL_INPUT_A, wire);
    }

    /**
     * Detaches input A from a wire.
     *
     * @param wire the wire
     */
    public void detachInputA(Wire wire) {
        detachTerminal(TERMINAL_INPUT_A, wire);
    }

    /**
     * Attaches input B to a wire.
     *
     * @param wire the wire
     */
    public void attachInputB(Wire wire) {
        attachTerminal(TERMINAL_INPUT_B, wire);
    }

    /**
     * Detaches input B from a wire.
     *
     * @param wire the wire
     */
    public void detachInputB(Wire wire) {
        detachTerminal(TERMINAL_INPUT_B, wire);
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
     * Subscribes to input A voltage relative to VSS.
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeInputVoltageA(
        ObservationListener listener
    ) {
        return observe(OBSERVER_INPUT_A_VOLTAGE, listener);
    }

    /**
     * Subscribes to input B voltage relative to VSS.
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeInputVoltageB(
        ObservationListener listener
    ) {
        return observe(OBSERVER_INPUT_B_VOLTAGE, listener);
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
