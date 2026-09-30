package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

/**
 * Models an inductor between a positive terminal and a negative terminal.
 *
 * <p>Voltage is the positive-terminal voltage minus the negative-terminal voltage.
 * Positive current flows from the positive terminal to the negative terminal.</p>
 */
public final class Inductor extends Device {
    /**
     * The device type for Inductor.
     */
    public static final DeviceType<Inductor> TYPE = DeviceType.primitive(8, Inductor::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_INDUCTANCE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    private Inductor() {
    }

    /**
     * Creates an inductor.
     *
     * @param system the electrical system
     * @param inductance the inductance, in henries; the value must be finite and greater than zero
     *
     * @return the inductor
     */
    public static Inductor create(ElectricalSystem system, double inductance) {
        Inductor device = system.create(TYPE);

        device.setInductance(inductance);

        return device;
    }

    /**
     * Sets the inductance.
     *
     * @param inductance the inductance, in henries; the value must be finite and greater than zero
     */
    public void setInductance(double inductance) {
        setParameter(PARAMETER_INDUCTANCE, inductance);
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
     * Subscribes to the voltage across this inductor.
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
     * Subscribes to the current through this inductor.
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
