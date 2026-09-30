package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;

/**
 * Is the base class for an electrical device.
 *
 * <p>A custom electrical device extends this class and defines a shared
 * {@link DeviceType}.</p>
 *
 * <p>An {@link ElectricalSystem} binds each device that it creates. Plugin
 * code must not bind a device directly.</p>
 *
 * <p>Use the protected methods to set parameters, attach terminals,
 * detach terminals, and create observation subscriptions.</p>
 */
@SuppressWarnings("resource")
public abstract class Device {
    private @Nullable ElectricalSystem system;

    private int id;
    private int generation;

    /**
     * Creates an unbound device.
     *
     * <p>The electrical system binds the device after construction.</p>
     */
    protected Device() {
    }

    /**
     * Creates an observation subscription for this device.
     *
     * <p>The observer ID is the zero-based order in which the device
     * definition adds its observers.</p>
     *
     * <p>Avoid exposing observer IDs in a plugin API. Provide methods with
     * semantic names, such as {@code observeVoltage()} or
     * {@code observeControlVoltage()}.</p>
     *
     * @param observerId the observer ID
     * @param listener the observation listener
     *
     * @return the observation subscription
     *
     * @throws NullPointerException if {@code listener} is null
     * @throws IllegalStateException if the device is not bound or its system
     *     is not usable
     */
    protected final ObservationSubscription observe(int observerId, ObservationListener listener) {
        return requireBound().subscribe(this, observerId, listener);
    }

    final void bind(ElectricalSystem system, int id, int generation) {
        requireUnbound();

        this.system = Objects.requireNonNull(system, "system");
        this.id = id;
        this.generation = generation;
    }

    final boolean belongsTo(ElectricalSystem system) {
        return this.system == system;
    }

    protected final int id() {
        requireBound();
        return id;
    }

    final int generation() {
        requireBound();
        return generation;
    }

    /**
     * Sets one parameter of this device.
     *
     * <p>The parameter ID is the zero-based order in which the device
     * definition adds its parameters.</p>
     *
     * <p>If this method is called from an observation callback, the change
     * applies to the next tick.</p>
     *
     * @param parameterId the parameter ID
     * @param value the parameter value
     *
     * @throws IllegalStateException if the device or its system is not usable
     */
    protected final void setParameter(int parameterId, double value) {
        requireBound().setParameter(this, parameterId, value);
    }

    /**
     * Attaches one device terminal to a wire.
     *
     * <p>The terminal ID is the zero-based order in which the device
     * definition adds its terminals. The wire must belong to the same
     * electrical system as this device.</p>
     *
     * @param terminalId the terminal ID
     * @param wire the wire
     *
     * @throws IllegalArgumentException if the wire belongs to another system
     * @throws IllegalStateException if the device or its system is not usable
     */
    protected final void attachTerminal(int terminalId, Wire wire) {
        requireBound().attachTerminal(this, terminalId, wire);
    }

    /**
     * Detaches one device terminal from a wire.
     *
     * <p>The wire must belong to the same electrical system as this device.</p>
     *
     * @param terminalId the terminal ID
     * @param wire the wire
     *
     * @throws IllegalArgumentException if the wire belongs to another system
     * @throws IllegalStateException if the device or its system is not usable
     */
    protected final void detachTerminal(int terminalId, Wire wire) {
        requireBound().detachTerminal(this, terminalId, wire);
    }

    /**
     * Removes this device from its electrical system.
     *
     * <p>This operation makes all observation subscriptions for this device
     * inactive. Do not use this device after this method completes.</p>
     *
     * @throws IllegalStateException if the device or its system is not usable
     */
    public final void destroy() {
        requireBound().remove(this);
    }

    final void requireUnbound() {
        if (system != null) {
            throw new IllegalStateException("Electrical device is already bound");
        }
    }

    private ElectricalSystem requireBound() {
        ElectricalSystem system = this.system;

        if (system == null) {
            throw new IllegalStateException("Electrical device is not bound");
        }

        return system;
    }
}