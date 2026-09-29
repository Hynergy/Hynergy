package dev.hynergy.electrical;

import org.jspecify.annotations.Nullable;

import java.util.Objects;
import java.util.function.Consumer;
import java.util.function.Supplier;

public final class DeviceType<T extends Device> {
    private final Supplier<T> constructor;
    private final @Nullable DeviceDefinition primitiveDefinition;
    private final @Nullable Consumer<DeviceDefinitionBuilder> definitionBuilder;

    private volatile @Nullable Registration registration;

    private DeviceType(
        Supplier<T> constructor,
        @Nullable DeviceDefinition primitiveDefinition,
        @Nullable Consumer<DeviceDefinitionBuilder> definitionBuilder
    ) {
        this.constructor = Objects.requireNonNull(constructor, "constructor");
        this.primitiveDefinition = primitiveDefinition;
        this.definitionBuilder = definitionBuilder;
    }

    public static <T extends Device> DeviceType<T> create(
        Supplier<T> constructor,
        Consumer<DeviceDefinitionBuilder> definitionBuilder
    ) {
        return new DeviceType<>(
            constructor,
            null,
            Objects.requireNonNull(definitionBuilder, "definitionBuilder")
        );
    }

    static <T extends Device> DeviceType<T> primitive(
        int definitionId,
        Supplier<T> constructor
    ) {
        return new DeviceType<>(
            constructor,
            new DeviceDefinition(definitionId),
            null
        );
    }

    T construct() {
        return Objects.requireNonNull(
            constructor.get(),
            "Device constructor returned null"
        );
    }

    void buildDefinition(DeviceDefinitionBuilder builder) {
        Consumer<DeviceDefinitionBuilder> definitionBuilder = this.definitionBuilder;

        if (definitionBuilder == null) {
            throw new IllegalStateException("Primitive device type does not have a composite definition");
        }

        definitionBuilder.accept(Objects.requireNonNull(builder, "builder"));
    }

    @Nullable
    DeviceDefinition currentDefinition() {
        DeviceDefinition primitiveDefinition = this.primitiveDefinition;

        if (primitiveDefinition != null) {
            return primitiveDefinition;
        }

        Registration registration = this.registration;

        return registration == null ? null : registration.definition();
    }

    @Nullable
    DeviceDefinition existingDefinition(ElectricalRuntime runtime) {
        Objects.requireNonNull(runtime, "runtime");

        DeviceDefinition primitiveDefinition = this.primitiveDefinition;

        if (primitiveDefinition != null) {
            return primitiveDefinition;
        }

        Registration registration = this.registration;

        if (registration == null || registration.runtime() != runtime) {
            return null;
        }

        return registration.definition();
    }

    DeviceDefinition requireDefinition(ElectricalRuntime runtime) {
        DeviceDefinition definition = existingDefinition(runtime);

        if (definition == null) {
            throw new IllegalStateException("Device type is not registered for this electrical runtime");
        }

        return definition;
    }

    synchronized void bind(
        ElectricalRuntime runtime,
        DeviceDefinition definition
    ) {
        Objects.requireNonNull(runtime, "runtime");
        Objects.requireNonNull(definition, "definition");

        if (primitiveDefinition != null) {
            throw new IllegalStateException("Primitive device type cannot be runtime-bound");
        }

        Registration registration = this.registration;

        if (registration != null && registration.runtime() == runtime) {
            throw new IllegalStateException("Device type is already bound to this electrical runtime");
        }

        this.registration = new Registration(runtime, definition);
    }

    synchronized void unbind(ElectricalRuntime runtime) {
        Registration registration = this.registration;

        if (registration != null && registration.runtime() == runtime) {
            this.registration = null;
        }
    }

    private record Registration(
        ElectricalRuntime runtime,
        DeviceDefinition definition
    ) {
    }
}
