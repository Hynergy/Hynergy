package dev.hynergy.electrical.primitives.controlled;

import dev.hynergy.electrical.*;

/**
 * Models a voltage-controlled conductance between two output terminals.
 *
 * <p>The control voltage is the control-terminal voltage minus the
 * output-negative-terminal voltage. The output conductance changes from the
 * minimum conductance to the maximum conductance across a transition interval
 * that is centered on the threshold voltage.</p>
 *
 * <p>Positive output current flows from output-positive to output-negative.</p>
 */
public final class VoltageControlledConductance extends Device {
    /**
     * The device type for {@code VoltageControlledConductance}.
     */
    public static final DeviceType<VoltageControlledConductance> TYPE =
        DeviceType.primitive(10, VoltageControlledConductance::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL = 2;

    private static final int PARAMETER_THRESHOLD_VOLTAGE = 0;
    private static final int PARAMETER_TRANSITION_VOLTAGE = 1;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 2;
    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 3;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_CONTROL_VOLTAGE = 1;
    private static final int OBSERVER_OUTPUT_CURRENT = 2;

    private VoltageControlledConductance() {
    }

    /**
     * Creates a voltage-controlled conductance.
     *
     * @param system the electrical system
     * @param thresholdVoltage the center of the transition interval, in volts;
     *     the value must be finite
     * @param transitionVoltage the width of the transition interval, in volts;
     *     the value must be finite and greater than zero
     * @param minimumConductance the minimum conductance, in siemens; the value
     *     must be finite and non-negative
     * @param maximumConductance the maximum conductance, in siemens; the value
     *     must be finite, greater than zero, and greater than
     *     {@code minimumConductance}
     *
     * @return the voltage-controlled conductance
     */
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

    /**
     * Sets the center of the transition interval.
     *
     * @param thresholdVoltage the threshold voltage, in volts; the value must
     *     be finite
     */
    public void setThresholdVoltage(double thresholdVoltage) {
        setParameter(PARAMETER_THRESHOLD_VOLTAGE, thresholdVoltage);
    }

    /**
     * Sets the width of the transition interval.
     *
     * @param transitionVoltage the transition voltage, in volts; the value
     *     must be finite and greater than zero
     */
    public void setTransitionVoltage(double transitionVoltage) {
        setParameter(PARAMETER_TRANSITION_VOLTAGE, transitionVoltage);
    }

    /**
     * Sets the minimum conductance.
     *
     * @param minimumConductance the conductance, in siemens; the value must be
     *     finite, non-negative, and less than the current maximum conductance
     */
    public void setMinimumConductance(double minimumConductance) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, minimumConductance);
    }

    /**
     * Sets the maximum conductance.
     *
     * @param maximumConductance the conductance, in siemens; the value must be
     *     finite, greater than zero, and greater than the current minimum
     *     conductance
     */
    public void setMaximumConductance(double maximumConductance) {
        setParameter(PARAMETER_MAXIMUM_CONDUCTANCE, maximumConductance);
    }

    /**
     * Attaches the output-positive terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachOutputPositive(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    /**
     * Detaches the output-positive terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachOutputPositive(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    /**
     * Attaches the output-negative terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachOutputNegative(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }

    /**
     * Detaches the output-negative terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachOutputNegative(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }

    /**
     * Attaches the control terminal to a wire.
     *
     * <p>The control voltage uses the output-negative terminal as its
     * reference.</p>
     *
     * @param wire the wire
     */
    public void attachControl(Wire wire) {
        attachTerminal(TERMINAL_CONTROL, wire);
    }

    /**
     * Detaches the control terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachControl(Wire wire) {
        detachTerminal(TERMINAL_CONTROL, wire);
    }

    /**
     * Subscribes to the output voltage.
     *
     * <p>The output voltage is the output-positive voltage minus the
     * output-negative voltage.</p>
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
     * Subscribes to the control voltage.
     *
     * <p>The control voltage is the control-terminal voltage minus the
     * output-negative-terminal voltage.</p>
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeControlVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_CONTROL_VOLTAGE, listener);
    }

    /**
     * Subscribes to the output current.
     *
     * <p>Positive output current flows from output-positive to
     * output-negative.</p>
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeOutputCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_CURRENT, listener);
    }
}
