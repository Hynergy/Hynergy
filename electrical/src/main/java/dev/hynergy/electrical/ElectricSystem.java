package dev.hynergy.electrical;

public final class ElectricSystem implements AutoCloseable {
    private final ElectricalRuntime runtime;
    private final ElectricalWorld world;

    private boolean closed;

    ElectricSystem(ElectricalRuntime runtime, ElectricalWorld world) {
        this.runtime = runtime;
        this.world = world;
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