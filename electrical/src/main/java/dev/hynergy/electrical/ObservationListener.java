package dev.hynergy.electrical;

/**
 * Receives updates for one electrical observation.
 *
 * <p>The first completed tick after subscription sends an update.
 * A later tick sends an update when availability changes or when an
 * available value changes.</p>
 *
 * <p>The listener runs during {@link ElectricalSystem#tick()}.
 * The listener can change the electrical system. The change applies to
 * the next tick.</p>
 *
 * <p>Do not call {@link ElectricalSystem#tick()} or
 * {@link ElectricalSystem#close()} from this listener.</p>
 *
 * <p>If this listener throws a runtime exception or an error, publication
 * continues for the other records.</p>
 */
@FunctionalInterface
public interface ObservationListener {

    /**
     * Receives one observation update.
     *
     * @param status the availability of the observed value
     * @param value the observed value when {@code status} is
     *     {@link ObservationStatus#AVAILABLE}
     */
    void onUpdate(ObservationStatus status, double value);
}