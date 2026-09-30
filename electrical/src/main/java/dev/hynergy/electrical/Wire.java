package dev.hynergy.electrical;

/**
 * Identifies one wire in an {@link ElectricSystem}.
 *
 * <p>A wire connects device terminals to the same electrical network.
 * The wire belongs to the system that created it.</p>
 */
public final class Wire {
    private final ElectricSystem system;
    private final int id;
    private final int generation;

    Wire(ElectricSystem system, int id, int generation) {
        this.system = system;
        this.id = id;
        this.generation = generation;
    }

    boolean belongsTo(ElectricSystem system) {
        return this.system == system;
    }

    int id() {
        return id;
    }

    int generation() {
        return generation;
    }
}
