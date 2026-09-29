package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;

@SuppressWarnings("resource")
public abstract class Device {
    private @Nullable ElectricSystem system;

    private int id;
    private int generation;

    protected Device() {
    }

    final void requireUnbound() {
        if (system != null) {
            throw new IllegalStateException("Electrical device is already bound");
        }
    }

    final void bind(ElectricSystem system, int id, int generation) {
        requireUnbound();

        this.system = Objects.requireNonNull(system, "system");
        this.id = id;
        this.generation = generation;
    }

    final boolean belongsTo(ElectricSystem system) {
        return this.system == system;
    }

    final int id() {
        requireBound();
        return id;
    }

    final int generation() {
        requireBound();
        return generation;
    }

    public final void remove() {
        requireBound().remove(this);
    }

    protected final void setParameter(int parameterId, double value) {
        requireBound().setParameter(this, parameterId, value);
    }

    protected final void attachTerminal(int terminalId, Wire wire) {
        requireBound().attachTerminal(this, terminalId, wire);
    }

    protected final void detachTerminal(int terminalId, Wire wire) {
        requireBound().detachTerminal(this, terminalId, wire);
    }

    private ElectricSystem requireBound() {
        ElectricSystem system = this.system;

        if (system == null) {
            throw new IllegalStateException("Electrical device is not bound");
        }

        return system;
    }
}