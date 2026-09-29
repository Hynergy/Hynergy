package dev.hynergy.electrical;

import java.util.IdentityHashMap;

public final class ElectricalRuntime implements AutoCloseable {

    private final ElectricalEngine engine;

    private boolean closed;
    private int systemCount;

    private final IdentityHashMap<DeviceType<?>, DeviceDefinition> deviceDefinitions = new IdentityHashMap<>();



    private ElectricalRuntime(ElectricalEngine engine) {
        this.engine = engine;
    }

    public static ElectricalRuntime create() {
        return new ElectricalRuntime(ElectricalEngine.create());
    }

    

    public synchronized <T extends Device> DeviceDefinition register(
        DeviceType<T> type
    ) {
        requireOpen();

        DeviceDefinition existing = deviceDefinitions.get(type);

        if (existing != null) {
            return existing;
        }

        DeviceDefinition definition = type.register(this);

        deviceDefinitions.put(type, definition);

        return definition;
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



    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical runtime is closed");
        }
    }

    DeviceDefinition requireDefinition(DeviceType<?> type) {
        DeviceDefinition definition = deviceDefinitions.get(type);

        if (definition == null) {
            throw new IllegalStateException("Device type is not registered");
        }

        return definition;
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
}