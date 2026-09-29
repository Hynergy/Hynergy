package dev.hynergy.electrical;

import java.util.function.Function;
import java.util.function.Supplier;

public final class DeviceType<T extends Device> {

    private final Supplier<T> constructor;
    private final Function<ElectricalRuntime, DeviceDefinition> registrar;

    private DeviceType(Supplier<T> constructor, Function<ElectricalRuntime, DeviceDefinition> registrar) {
        this.constructor = constructor;
        this.registrar = registrar;
    }

    public static <T extends Device> DeviceType<T> create(
        Supplier<T> constructor,
        Function<ElectricalRuntime, DeviceDefinition> registrar
    ) {
        return new DeviceType<>(constructor, registrar);
    }

    T construct() {
        return constructor.get();
    }

    DeviceDefinition register(ElectricalRuntime runtime) {
        return registrar.apply(runtime);
    }
}
