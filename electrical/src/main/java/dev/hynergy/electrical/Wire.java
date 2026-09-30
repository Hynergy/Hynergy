package dev.hynergy.electrical;

/**
 * Identifies one wire in an {@link ElectricalSystem}.
 *
 * <p>A wire connects device terminals to the same electrical network.
 * The wire belongs to the system that created it.</p>
 */
public final class Wire {
    private final ElectricalSystem system;
    private final int id;
    private final int generation;

    Wire(ElectricalSystem system, int id, int generation) {
        this.system = system;
        this.id = id;
        this.generation = generation;
    }

    boolean belongsTo(ElectricalSystem system) {
        return this.system == system;
    }

    int id() {
        return id;
    }

    int generation() {
        return generation;
    }
}
