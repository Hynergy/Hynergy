package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.*;

final class ElectricalRuntimeTest {
    private static final int RESISTANCE_DEFINITION_ID = 1;

    @Test
    void registrationResolvesDependenciesAndCachesDefinitions() {
        DeviceType<TestDevice> resistance = resistanceType();
        DeviceType<TestDevice> child = resistorComposite(resistance);
        DeviceType<TestDevice> parent = forwardingComposite(child);

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            DeviceDefinition parentDefinition = runtime.register(parent);
            DeviceDefinition childDefinition = runtime.requireDefinition(child);

            assertNotEquals(0, parentDefinition.id());
            assertNotEquals(0, childDefinition.id());
            assertNotEquals(parentDefinition.id(), childDefinition.id());
            assertSame(parentDefinition, runtime.register(parent));
        }
    }

    @Test
    void registrationRejectsRecursiveDependenciesAndRemainsUsable() {
        AtomicReference<DeviceType<TestDevice>> firstReference = new AtomicReference<>();
        AtomicReference<DeviceType<TestDevice>> secondReference = new AtomicReference<>();

        DeviceType<TestDevice> first = DeviceType.create(
            TestDevice::new,
            builder -> builder.beginElement(secondReference.get())
        );
        DeviceType<TestDevice> second = DeviceType.create(
            TestDevice::new,
            builder -> builder.beginElement(firstReference.get())
        );

        firstReference.set(first);
        secondReference.set(second);

        DeviceType<TestDevice> valid = resistorComposite(resistanceType());

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            assertThrows(
                IllegalStateException.class,
                () -> runtime.register(first)
            );

            assertDoesNotThrow(() -> runtime.register(valid));
        }
    }

    @Test
    void failedRootRegistrationKeepsSuccessfulDependencies() {
        DeviceType<TestDevice> child = resistorComposite(resistanceType());
        DeviceType<TestDevice> invalidParent = DeviceType.create(
            TestDevice::new,
            builder -> builder.beginElement(child).endElement()
        );

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            assertThrows(
                IllegalArgumentException.class,
                () -> runtime.register(invalidParent)
            );

            DeviceDefinition childDefinition = runtime.requireDefinition(child);

            assertSame(childDefinition, runtime.register(child));
        }
    }

    @Test
    void unregisteredTypesCannotBeAddedWhileSystemsAreActive() {
        DeviceType<TestDevice> type = resistorComposite(resistanceType());

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            ElectricSystem system = runtime.createSystem(20);

            try (system) {
                assertThrows(
                    IllegalStateException.class,
                    () -> runtime.register(type)
                );
            }

            assertDoesNotThrow(() -> runtime.register(type));
        }
    }

    @Test
    void registeredDefinitionIsClearedWhenRuntimeCloses() {
        DeviceType<TestDevice> type = resistorComposite(resistanceType());

        DeviceDefinition firstDefinition;

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            firstDefinition = runtime.register(type);
        }

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            DeviceDefinition secondDefinition = runtime.register(type);

            assertNotSame(firstDefinition, secondDefinition);
        }
    }


    @Test
    void staleRuntimeCleanupDoesNotClearNewRuntimeBinding() {
        DeviceType<TestDevice> type = resistorComposite(resistanceType());

        ElectricalRuntime firstRuntime = ElectricalRuntime.create();

        firstRuntime.register(type);
        firstRuntime.close();

        try (ElectricalRuntime secondRuntime = ElectricalRuntime.create()) {
            DeviceDefinition definition = secondRuntime.register(type);

            type.unbind(firstRuntime);

            assertSame(definition, secondRuntime.requireDefinition(type));
        }
    }

    @Test
    void reusedDeviceInstanceIsRejectedBeforeWorldIdAllocation() {
        TestDevice reused = new TestDevice();
        AtomicInteger constructorCalls = new AtomicInteger();

        DeviceType<TestDevice> type = DeviceType.create(
            () -> constructorCalls.getAndIncrement() < 2 ? reused : new TestDevice(),
            builder -> {
                int firstTerminal = builder.addTerminal();
                int secondTerminal = builder.addTerminal();

                builder.beginElement(resistanceType())
                    .elementTerminal(firstTerminal)
                    .elementTerminal(secondTerminal)
                    .elementLiteral(1_000.0)
                    .endElement();
            }
        );

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            runtime.register(type);

            try (ElectricSystem system = runtime.createSystem(20)) {
                TestDevice first = system.create(type);

                assertEquals(1, first.id());
                assertThrows(IllegalStateException.class, () -> system.create(type));

                TestDevice second = system.create(type);

                assertEquals(2, second.id());
            }
        }
    }

    @Test
    void primitiveTypesHaveFixedDefinitions() {
        DeviceType<TestDevice> resistance = resistanceType();

        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            DeviceDefinition definition = runtime.register(resistance);

            assertEquals(RESISTANCE_DEFINITION_ID, definition.id());
            assertSame(definition, runtime.register(resistance));
        }
    }

    private static DeviceType<TestDevice> resistanceType() {
        return DeviceType.primitive(
            RESISTANCE_DEFINITION_ID,
            TestDevice::new
        );
    }

    private static DeviceType<TestDevice> resistorComposite(
        DeviceType<?> resistance
    ) {
        return DeviceType.create(
            TestDevice::new,
            builder -> {
                int firstTerminal = builder.addTerminal();
                int secondTerminal = builder.addTerminal();

                builder.beginElement(resistance)
                    .elementTerminal(firstTerminal)
                    .elementTerminal(secondTerminal)
                    .elementLiteral(1_000.0)
                    .endElement();
            }
        );
    }

    private static DeviceType<TestDevice> forwardingComposite(
        DeviceType<?> child
    ) {
        return DeviceType.create(
            TestDevice::new,
            builder -> {
                int firstTerminal = builder.addTerminal();
                int secondTerminal = builder.addTerminal();

                builder.beginElement(child)
                    .elementTerminal(firstTerminal)
                    .elementTerminal(secondTerminal)
                    .endElement();
            }
        );
    }

    private static final class TestDevice extends Device {
    }
}
