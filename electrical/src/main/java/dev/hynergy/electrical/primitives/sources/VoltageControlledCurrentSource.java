package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.*;

/**
 * Models a voltage-controlled current source.
 *
 * <p>The control voltage is the control-positive voltage minus the
 * control-negative voltage. The output current is the control voltage
 * multiplied by the transconductance. Positive output current flows from
 * output-positive to output-negative.</p>
 */
public final class VoltageControlledCurrentSource extends Device {
    /**
     * The device type for {@code VoltageControlledCurrentSource}.
     */
    public static final DeviceType<VoltageControlledCurrentSource> TYPE =
        DeviceType.primitive(5, VoltageControlledCurrentSource::new);

    private static final int TERMINAL_OUTPUT_POSITIVE = 0;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 1;
    private static final int TERMINAL_CONTROL_POSITIVE = 2;
    private static final int TERMINAL_CONTROL_NEGATIVE = 3;

    private static final int PARAMETER_TRANSCONDUCTANCE = 0;

    private static final int OBSERVER_OUTPUT_VOLTAGE = 0;
    private static final int OBSERVER_CONTROL_VOLTAGE = 1;
    private static final int OBSERVER_OUTPUT_CURRENT = 2;

    private VoltageControlledCurrentSource() {
    }

    /**
     * Creates a voltage-controlled current source.
     *
     * @param system the electrical system
     * @param transconductance the transconductance, in siemens; the value must
     *     be finite
     *
     * @return the voltage-controlled current source
     */
    public static VoltageControlledCurrentSource create(ElectricalSystem system, double transconductance) {
        VoltageControlledCurrentSource device = system.create(TYPE);

        device.setTransconductance(transconductance);

        return device;
    }

    /**
     * Sets the transconductance.
     *
     * @param transconductance the transconductance, in siemens; the value must
     *     be finite
     */
    public void setTransconductance(double transconductance) {
        setParameter(PARAMETER_TRANSCONDUCTANCE, transconductance);
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
     * Attaches the control-positive terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachControlPositive(Wire wire) {
        attachTerminal(TERMINAL_CONTROL_POSITIVE, wire);
    }

    /**
     * Detaches the control-positive terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachControlPositive(Wire wire) {
        detachTerminal(TERMINAL_CONTROL_POSITIVE, wire);
    }

    /**
     * Attaches the control-negative terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachControlNegative(Wire wire) {
        attachTerminal(TERMINAL_CONTROL_NEGATIVE, wire);
    }

    /**
     * Detaches the control-negative terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachControlNegative(Wire wire) {
        detachTerminal(TERMINAL_CONTROL_NEGATIVE, wire);
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
     * <p>The control voltage is the control-positive voltage minus the
     * control-negative voltage.</p>
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
