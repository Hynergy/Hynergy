package dev.hynergy.electrical;

import dev.hynergy.electrical.primitives.passive.Resistance;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class ElectricSystemObservationTest {
    private static final ObservationListener NOOP_LISTENER = (status, value) -> {
    };

    @Test
    void subscribeReturnsActiveHandle() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            ObservationSubscription subscription = system.subscribe(resistor, 0, NOOP_LISTENER);

            assertTrue(subscription.isActive());
            assertNotEquals(0, subscription.nativeId());
            assertEquals(resistor.id(), subscription.deviceId());
            assertSame(NOOP_LISTENER, subscription.listener());
        }
    }

    @Test
    void unsubscribeIsIdempotent() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            ObservationSubscription subscription = system.subscribe(resistor, 0, NOOP_LISTENER);

            subscription.unsubscribe();

            assertFalse(subscription.isActive());

            assertThrows(IllegalStateException.class, subscription::listener);

            assertDoesNotThrow(subscription::unsubscribe);
            assertFalse(subscription.isActive());
        }
    }

    @Test
    void removingDeviceInvalidatesAllItsSubscriptions() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            ObservationSubscription voltage = system.subscribe(resistor, 0, NOOP_LISTENER);
            ObservationSubscription current = system.subscribe(resistor, 1, NOOP_LISTENER);

            resistor.remove();

            assertFalse(voltage.isActive());
            assertFalse(current.isActive());

            assertThrows(IllegalStateException.class, voltage::listener);
            assertThrows(IllegalStateException.class, current::listener);

            assertDoesNotThrow(voltage::unsubscribe);
            assertDoesNotThrow(current::unsubscribe);
        }
    }

    @Test
    void removingDeviceDoesNotInvalidateOtherDeviceSubscriptions() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance first = Resistance.create(system, 10.0);

            Resistance second = Resistance.create(system, 20.0);

            ObservationSubscription firstSubscription = system.subscribe(first, 0, NOOP_LISTENER);
            ObservationSubscription secondSubscription = system.subscribe(second, 0, NOOP_LISTENER);

            first.remove();

            assertFalse(firstSubscription.isActive());
            assertTrue(secondSubscription.isActive());

            assertSame(NOOP_LISTENER, secondSubscription.listener());
        }
    }

    @Test
    void invalidObserverDoesNotPoisonSystem() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            assertThrows(
                ElectricalWorld.SubscriptionOperationException.class,
                () -> system.subscribe(resistor, Integer.MAX_VALUE, NOOP_LISTENER)
            );

            ObservationSubscription valid = assertDoesNotThrow(() -> system.subscribe(resistor, 0, NOOP_LISTENER));

            assertTrue(valid.isActive());
        }
    }

    @Test
    void subscriptionCannotBeUnsubscribedThroughAnotherSystem() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create();
            ElectricSystem first = runtime.createSystem(20);
            ElectricSystem second = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(first, 10.0);

            ObservationSubscription subscription = first.subscribe(resistor, 0, NOOP_LISTENER);

            assertThrows(IllegalArgumentException.class, () -> second.unsubscribe(subscription));
            assertTrue(subscription.isActive());

            subscription.unsubscribe();

            assertFalse(subscription.isActive());
        }
    }

    @Test
    void closingSystemInvalidatesAllSubscriptions() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create()) {
            ElectricSystem system = runtime.createSystem(20);

            Resistance resistor = Resistance.create(system, 10.0);

            ObservationSubscription voltage = system.subscribe(resistor, 0, NOOP_LISTENER);
            ObservationSubscription current = system.subscribe(resistor, 1, NOOP_LISTENER);

            system.close();

            assertFalse(voltage.isActive());
            assertFalse(current.isActive());

            assertThrows(IllegalStateException.class, voltage::listener);
            assertThrows(IllegalStateException.class, current::listener);

            assertDoesNotThrow(system::close);
        }
    }
}