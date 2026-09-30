package dev.hynergy.electrical;

import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.Objects;

/**
 * Owns the electrical engine and its device type registrations.
 *
 * <p>Create one runtime before you create an {@link ElectricSystem}.
 * Register custom device types before you create a system.</p>
 *
 * <p>Close all systems before you close the runtime.</p>
 *
 * <p>Only one electrical runtime can be active in the process.</p>
 */
public final class ElectricalRuntime implements AutoCloseable {
    private final ElectricalEngine engine;

    private final ArrayList<DeviceType<?>> boundTypes = new ArrayList<>();
    private final IdentityHashMap<DeviceType<?>, Boolean> registeringTypes = new IdentityHashMap<>();

    private boolean closed;
    private int systemCount;

    private ElectricalRuntime(ElectricalEngine engine) {
        this.engine = engine;
    }

    /**
     * Creates the electrical runtime.
     *
     * @return the new electrical runtime
     *
     * @throws IllegalStateException if another electrical runtime is active
     *     or if the electrical engine cannot start
     */
    public static ElectricalRuntime create() {
        return new ElectricalRuntime(ElectricalEngine.create());
    }

    /**
     * Registers a device type in this runtime.
     *
     * <p>Register custom device types before you create an electrical system.
     * Registration also resolves child device types that the definition uses.</p>
     *
     * <p>If this runtime already contains the device type, this method returns
     * its registered definition.</p>
     *
     * @param type the device type
     * @param <T> the device class
     *
     * @return the registered device definition
     *
     * @throws NullPointerException if {@code type} is null
     * @throws IllegalStateException if the runtime is closed, if a system is
     *     active, or if registration fails
     */
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
            throw new IllegalStateException("Device types cannot be registered while electrical systems are active");
        }

        return resolveDefinition(type);
    }

    /**
     * Creates an electrical system.
     *
     * @param tickFrequencyHz the simulation tick frequency, in hertz
     *
     * @return the new electrical system
     *
     * @throws IllegalArgumentException if {@code tickFrequencyHz} is not
     *     greater than zero
     * @throws IllegalStateException if the runtime is closed or device type
     *     registration is in progress
     */
    public synchronized ElectricSystem createSystem(int tickFrequencyHz) {
        requireOpen();

        if (!registeringTypes.isEmpty()) {
            throw new IllegalStateException(
                "Electrical system creation is unavailable during device type " + "registration");
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

    /**
     * Closes this runtime and releases its resources.
     *
     * <p>A second call after a successful close has no effect.</p>
     *
     * @throws IllegalStateException if a system is active or device type
     *     registration is in progress
     */
    @Override
    public synchronized void close() {
        if (closed) {
            return;
        }

        if (systemCount != 0) {
            throw new IllegalStateException("Electrical runtime has active systems");
        }

        if (!registeringTypes.isEmpty()) {
            throw new IllegalStateException("Electrical runtime cannot close during device type registration");
        }

        engine.close();

        for (int index = boundTypes.size() - 1; index >= 0; index--) {
            boundTypes.get(index).unbind(this);
        }

        boundTypes.clear();
        closed = true;
    }
}
