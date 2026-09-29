package dev.hynergy.electrical;

public final class ElectricSystem implements AutoCloseable {
    private final ElectricalRuntime runtime;
    private final ElectricalWorld world;

    private boolean closed;

    ElectricSystem(ElectricalRuntime runtime, ElectricalWorld world) {
        this.runtime = runtime;
        this.world = world;
    }

    public Wire createWire() {
        requireOpen();

        int id = world.addWire();
        int generation = world.wireGeneration(id);

        return new Wire(this, id, generation);
    }

    void connect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.connectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void disconnect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.disconnectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void remove(Wire wire) {
        requireOwned(wire);

        world.removeWire(wire.id(), wire.generation());
    }

    private void requireOwned(Wire wire) {
        requireOpen();

        if (!wire.belongsTo(this)) {
            throw new IllegalArgumentException("Wire belongs to another electrical system");
        }
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical system is closed");
        }
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }

        world.close();
        closed = true;

        runtime.releaseSystem();
    }
}