package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

/**
 * Models a resistor between a positive terminal and a negative terminal.
 *
 * <p>Voltage is the positive-terminal voltage minus the negative-terminal voltage.
 * Positive current flows from the positive terminal to the negative terminal.</p>
 */
public final class Resistance extends Device {
    /**
     * The device type for Resistance.
     */
    public static final DeviceType<Resistance> TYPE = DeviceType.primitive(1, Resistance::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_RESISTANCE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    private Resistance() {
    }

    /**
     * Creates a resistor.
     *
     * @param system the electrical system
     * @param resistance the resistance, in ohms; the value must be finite and greater than zero
     *
     * @return the resistor
     */
    public static Resistance create(ElectricalSystem system, double resistance) {
        Resistance device = system.create(TYPE);

        device.setResistance(resistance);

        return device;
    }

    /**
     * Sets the resistance.
     *
     * @param resistance the resistance, in ohms; the value must be finite and greater than zero
     */
    public void setResistance(double resistance) {
        setParameter(PARAMETER_RESISTANCE, resistance);
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
     * Subscribes to the voltage across this resistor.
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
     * Subscribes to the current through this resistor.
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
