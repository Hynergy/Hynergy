package dev.hynergy.electrical;

import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.Objects;

/**
 * Owns the electrical engine and its device type registrations.
 *
 * <p>Create one runtime before you create an {@link ElectricalSystem}.
 * Register custom device types before you create a system.</p>
 *
 * <p>Close all electrical systems before you call {@link #close()}, or call
 * {@link #requestClose()} to close the runtime automatically after its active
 * systems close.</p>
 *
 * <p>Only one electrical runtime can be active in the process.</p>
 */
public final class ElectricalRuntime implements AutoCloseable {
    private final ElectricalEngine engine;

    private final ArrayList<DeviceType<?>> boundTypes = new ArrayList<>();
    private final IdentityHashMap<DeviceType<?>, Boolean> registeringTypes = new IdentityHashMap<>();

    private boolean closeRequested;
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
     * @throws IllegalStateException if the runtime is closed, if closing has
     *     been requested, if an electrical system is active, or if registration
     *     fails
     */
    public synchronized <T extends Device> DeviceDefinition register(
        DeviceType<T> type
    ) {
        requireOpen();
        requireActive();

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
     * @throws IllegalStateException if the runtime is closed, if closing has
     *     been requested, or if device type registration is in progress
     */
    public synchronized ElectricalSystem createSystem(int tickFrequencyHz) {
        requireOpen();
        requireActive();

        if (!registeringTypes.isEmpty()) {
            throw new IllegalStateException(
                "Electrical system creation is unavailable during device type " + "registration");
        }

        ElectricalWorld world = engine.createWorld(tickFrequencyHz);

        systemCount++;

        return new ElectricalSystem(this, world);
    }

    synchronized void releaseSystem() {
        if (systemCount <= 0) {
            throw new IllegalStateException("Electrical runtime system count is invalid");
        }

        systemCount--;

        if (closeRequested && systemCount == 0) {
            close();
        }
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
                boundTypes.removeLast();
                throw failure;
            }

            return definition;
        } finally {
            registeringTypes.remove(type);
        }
    }

    /**
     * Requests this runtime to close after its active electrical systems close.
     *
     * <p>This method prevents new device registrations and new electrical
     * systems from being created. Existing electrical systems remain usable
     * until they are closed.</p>
     *
     * <p>If no electrical systems are active, this method closes the runtime
     * immediately. Otherwise, the runtime closes automatically when the last
     * active electrical system closes.</p>
     *
     * <p>A second call has no effect.</p>
     */
    public synchronized void requestClose() {
        if (closed || closeRequested) {
            return;
        }

        closeRequested = true;

        if (systemCount == 0) {
            close();
        }
    }

    /**
     * Closes this runtime and releases its resources.
     *
     * <p>This method closes the runtime immediately. All electrical systems
     * must be closed before this method is called. Use {@link #requestClose()}
     * when the runtime must close after its active electrical systems close.</p>
     *
     * <p>A second call after a successful close has no effect.</p>
     *
     * @throws IllegalStateException if an electrical system is active or device
     *     type registration is in progress
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

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical runtime is closed");
        }
    }

    private void requireActive() {
        if (closed) {
            throw new IllegalStateException("Electrical runtime is closed");
        }

        if (closeRequested) {
            throw new IllegalStateException("Electrical runtime is closing");
        }
    }
}
