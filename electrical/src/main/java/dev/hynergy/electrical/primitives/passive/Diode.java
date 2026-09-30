package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.*;

/**
 * Models a two-state conductance diode.
 *
 * <p>Voltage is the anode voltage minus the cathode voltage. Positive current
 * flows from the anode to the cathode.</p>
 *
 * <p>The diode uses the maximum conductance when the anode-to-cathode voltage
 * is positive. It uses the minimum conductance when the voltage is zero or
 * negative.</p>
 */
public final class Diode extends Device {
    /**
     * The device type for {@code Diode}.
     */
    public static final DeviceType<Diode> TYPE = DeviceType.primitive(12, Diode::new);

    private static final int TERMINAL_ANODE = 0;
    private static final int TERMINAL_CATHODE = 1;

    private static final int PARAMETER_MAXIMUM_CONDUCTANCE = 0;
    private static final int PARAMETER_MINIMUM_CONDUCTANCE = 1;

    private static final int OBSERVER_VOLTAGE = 0;
    private static final int OBSERVER_CURRENT = 1;

    private Diode() {
    }

    /**
     * Creates a diode.
     *
     * @param system the electrical system
     * @param maximumConductance the forward conductance, in siemens; the value
     *     must be finite and greater than zero
     * @param minimumConductance the reverse conductance, in siemens; the value
     *     must be finite and non-negative, and less than
     *     {@code maximumConductance}
     *
     * @return the diode
     */
    public static Diode create(ElectricalSystem system, double maximumConductance, double minimumConductance) {
        Diode device = system.create(TYPE);

        device.setMaximumConductance(maximumConductance);
        device.setMinimumConductance(minimumConductance);

        return device;
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
     * Sets the minimum conductance.
     *
     * @param minimumConductance the conductance, in siemens; the value must be
     *     finite, non-negative, and less than the current maximum conductance
     */
    public void setMinimumConductance(double minimumConductance) {
        setParameter(PARAMETER_MINIMUM_CONDUCTANCE, minimumConductance);
    }

    /**
     * Attaches the anode to a wire.
     *
     * @param wire the wire
     */
    public void attachAnode(Wire wire) {
        attachTerminal(TERMINAL_ANODE, wire);
    }

    /**
     * Attaches the cathode to a wire.
     *
     * @param wire the wire
     */
    public void attachCathode(Wire wire) {
        attachTerminal(TERMINAL_CATHODE, wire);
    }

    /**
     * Detaches the anode from a wire.
     *
     * @param wire the wire
     */
    public void detachAnode(Wire wire) {
        detachTerminal(TERMINAL_ANODE, wire);
    }

    /**
     * Detaches the cathode from a wire.
     *
     * @param wire the wire
     */
    public void detachCathode(Wire wire) {
        detachTerminal(TERMINAL_CATHODE, wire);
    }

    /**
     * Subscribes to the voltage across this diode.
     *
     * <p>Positive voltage is measured from the anode to the cathode.</p>
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
     * Subscribes to the current through this diode.
     *
     * <p>Positive current flows from the anode to the cathode.</p>
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
