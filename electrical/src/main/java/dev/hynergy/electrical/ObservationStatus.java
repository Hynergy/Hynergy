package dev.hynergy.electrical;

/**
 * Specifies whether an observed electrical value is available.
 */
public enum ObservationStatus {
    /**
     * The observed value is available.
     *
     * <p>The value that the listener receives is valid.</p>
     */
    AVAILABLE,

    /**
     * The observed value is not available.
     *
     * <p>Do not use the value that the listener receives.</p>
     */
    UNAVAILABLE
}