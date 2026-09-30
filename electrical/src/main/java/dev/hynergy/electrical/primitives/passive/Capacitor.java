package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

/**
 * Models a capacitor between a positive terminal and a negative terminal.
 *
 * <p>Voltage is the positive-terminal voltage minus the negative-terminal voltage.
 * Positive current flows from the positive terminal to the negative terminal.</p>
 */
public final class Capacitor extends Device {
    /**
     * The device type for Capacitor.
     */
    public static final DeviceType<Capacitor> TYPE = DeviceType.primitive(7, Capacitor::new);

    private static final int TERMINAL_POSITIVE = 0;
    private static final int TERMINAL_NEGATIVE = 1;

    private static final int PARAMETER_CAPACITANCE = 0;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    private Capacitor() {
    }

    /**
     * Creates a capacitor.
     *
     * @param system the electrical system
     * @param capacitance the capacitance, in farads; the value must be finite and greater than zero
     *
     * @return the capacitor
     */
    public static Capacitor create(ElectricalSystem system, double capacitance) {
        Capacitor device = system.create(TYPE);

        device.setCapacitance(capacitance);

        return device;
    }

    /**
     * Sets the capacitance.
     *
     * @param capacitance the capacitance, in farads; the value must be finite and greater than zero
     */
    public void setCapacitance(double capacitance) {
        setParameter(PARAMETER_CAPACITANCE, capacitance);
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
     * Subscribes to the voltage across this capacitor.
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
     * Subscribes to the current through this capacitor.
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
