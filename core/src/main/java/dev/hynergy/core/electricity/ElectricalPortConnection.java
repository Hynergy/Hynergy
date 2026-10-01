package dev.hynergy.core.electricity;

/**
 * Snapshot relationship produced by electrical port discovery.
 *
 * <p>The value does not create or retain electrical topology; the consuming
 * block/integration decides how to translate it into electrical-system state.</p>
 */
public enum ElectricalPortConnection {
    DIRECT
}
