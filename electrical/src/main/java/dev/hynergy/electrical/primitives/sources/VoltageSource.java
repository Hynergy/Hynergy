package dev.hynergy.electrical.primitives.sources;

import dev.hynergy.electrical.*;

/**
 * Models an ideal voltage source between a positive terminal and a negative terminal.
 *
 * <p>Voltage is the positive-terminal voltage minus the negative-terminal voltage.
 * Positive current flows from the positive terminal to the negative terminal.</p>
 */
public final class VoltageSource extends Device {
    /**
     * The device type for VoltageSource.
     */
    public static final DeviceType<VoltageSource> TYPE = DeviceType.primitive(3, VoltageSource::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_VOLTAGE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    private VoltageSource() {
    }

    /**
     * Creates a voltage source.
     *
     * @param system the electrical system
     * @param voltage the source voltage, in volts; the value must be finite
     *
     * @return the voltage source
     */
    public static VoltageSource create(ElectricalSystem system, double voltage) {
        VoltageSource device = system.create(TYPE);

        device.setVoltage(voltage);

        return device;
    }

    /**
     * Sets the voltage.
     *
     * @param voltage the source voltage, in volts; the value must be finite
     */
    public void setVoltage(double voltage) {
        setParameter(PARAMETER_VOLTAGE, voltage);
    }

    /**
     * Attaches the positive terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachPositive(Wire wire) {
        attachTerminal(TERMINAL_POSITIVE, wire);
    }

    /**
     * Attaches the negative terminal to a wire.
     *
     * @param wire the wire
     */
    public void attachNegative(Wire wire) {
        attachTerminal(TERMINAL_NEGATIVE, wire);
    }

    /**
     * Detaches the positive terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachPositive(Wire wire) {
        detachTerminal(TERMINAL_POSITIVE, wire);
    }

    /**
     * Detaches the negative terminal from a wire.
     *
     * @param wire the wire
     */
    public void detachNegative(Wire wire) {
        detachTerminal(TERMINAL_NEGATIVE, wire);
    }

    /**
     * Subscribes to the voltage across this voltage source.
     *
     * <p>Positive voltage is measured from the positive terminal to the negative terminal.</p>
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_VOLTAGE, listener);
    }

    /**
     * Subscribes to the current through this voltage source.
     *
     * <p>Positive current flows from the positive terminal to the negative terminal.</p>
     *
     * @param listener the observation listener
     *
     * @return the observation subscription
     */
    public ObservationSubscription observeCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_CURRENT, listener);
    }
}
