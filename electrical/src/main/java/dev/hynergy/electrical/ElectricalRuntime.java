package dev.hynergy.electrical;

import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.Objects;

public final class ElectricalRuntime implements AutoCloseable {
    private final ElectricalEngine engine;

    private final ArrayList<DeviceType<?>> boundTypes = new ArrayList<>();
    private final IdentityHashMap<DeviceType<?>, Boolean> registeringTypes = new IdentityHashMap<>();

    private boolean closed;
    private int systemCount;

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
        Objects.requireNonNull(type, "type");

        DeviceDefinition existing = type.existingDefinition(this);

        if (existing != null) {
            return existing;
        }

        if (systemCount != 0) {
            throw new IllegalStateException(
                "Device types cannot be registered while electrical systems are active"
            );
        }

        return resolveDefinition(type);
    }

    public synchronized ElectricSystem createSystem(int tickFrequencyHz) {
        requireOpen();

        if (!registeringTypes.isEmpty()) {
            throw new IllegalStateException(
                "Electrical system creation is unavailable during device type registration"
            );
        }

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

    DeviceDefinition requireDefinition(DeviceType<?> type) {
        return Objects.requireNonNull(type, "type").requireDefinition(this);
    }

    private DeviceDefinition resolveDefinition(DeviceType<?> type) {
        DeviceDefinition existing = type.existingDefinition(this);

        if (existing != null) {
            return existing;
        }

        if (registeringTypes.put(type, Boolean.TRUE) != null) {
            throw new IllegalStateException("Recursive device type dependency");
        }

        try (DeviceDefinitionBuilder builder = new DeviceDefinitionBuilder(this::resolveDefinition)) {
            type.buildDefinition(builder);

            DeviceDefinition definition = engine.registerDefinition(builder);

            boundTypes.add(type);

            try {
                type.bind(this, definition);
            } catch (RuntimeException | Error failure) {
                boundTypes.remove(boundTypes.size() - 1);
                throw failure;
            }

            return definition;
        } finally {
            registeringTypes.remove(type);
        }
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical runtime is closed");
        }
    }

    @Override
    public synchronized void close() {
        if (closed) {
            return;
        }

        if (systemCount != 0) {
            throw new IllegalStateException("Electrical runtime has active systems");
        }

        if (!registeringTypes.isEmpty()) {
            throw new IllegalStateException(
                "Electrical runtime cannot close during device type registration"
            );
        }

        engine.close();

        for (int index = boundTypes.size() - 1; index >= 0; index--) {
            boundTypes.get(index).unbind(this);
        }

        boundTypes.clear();
        closed = true;
    }
}
