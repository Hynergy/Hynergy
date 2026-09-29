package dev.hynergy.electrical;

public final class ElectricalRuntime implements AutoCloseable {
    private final ElectricalEngine engine;

    private boolean closed;
    private int systemCount;

    private ElectricalRuntime(ElectricalEngine engine) {
        this.engine = engine;
    }

    public static ElectricalRuntime create() {
        return new ElectricalRuntime(ElectricalEngine.create());
    }

    public synchronized DeviceDefinition registerDefinition(
        DeviceDefinitionBuilder builder
    ) {
        requireOpen();
        return engine.registerDefinition(builder);
    }

    public synchronized ElectricSystem createSystem(int tickFrequencyHz) {
        requireOpen();

        ElectricalWorld world = engine.createWorld(tickFrequencyHz);

        systemCount++;

        return new ElectricSystem(this, world);
    }

    synchronized void releaseSystem() {
        if (systemCount <= 0) {
            throw new IllegalStateException("Electrical runtime system count is invalid");
        }

        systemCount--;
    }

    @Override
    public synchronized void close() {
        if (closed) {
            return;
        }

        if (systemCount != 0) {
            throw new IllegalStateException("Electrical runtime has active systems");
        }

        engine.close();
        closed = true;
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical runtime is closed");
        }
    }
}