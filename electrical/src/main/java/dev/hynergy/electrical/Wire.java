package dev.hynergy.electrical;

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
