/**
 * Provides the public electrical simulation API.
 *
 * <p>Create an {@link dev.hynergy.electrical.ElectricalRuntime} before
 * you create an electrical system. Register custom device types before
 * you create an electrical system.</p>
 *
 * <p>Create wires and devices in an
 * {@link dev.hynergy.electrical.ElectricSystem}. Call
 * {@link dev.hynergy.electrical.ElectricSystem#tick()} to advance the
 * simulation.</p>
 *
 * <p>An electrical system is thread-confined. Call its methods only from
 * the thread that created the system.</p>
 *
 * <p>Observation callbacks run during a system tick. A callback can change
 * the electrical system. The change applies to the next tick. Do not call
 * {@code tick()} or {@code close()} from an observation callback.</p>
 */
@NullMarked
package dev.hynergy.electrical;

import org.jspecify.annotations.NullMarked;