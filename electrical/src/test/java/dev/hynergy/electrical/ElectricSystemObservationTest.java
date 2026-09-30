package dev.hynergy.electrical;

import dev.hynergy.electrical.primitives.passive.Resistance;
import dev.hynergy.electrical.primitives.sources.VoltageSource;
import org.jspecify.annotations.Nullable;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;

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

    @Test
    void firstTickPublishesInitialObservation() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger callbacks = new AtomicInteger();

            system.subscribe(resistor, 0, (status, value) -> callbacks.incrementAndGet());
            system.tick();

            assertEquals(1, callbacks.get());
        }
    }

    @Test
    void unchangedTickDoesNotRepublishObservation() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger callbacks = new AtomicInteger();

            system.subscribe(resistor, 0, (status, value) -> callbacks.incrementAndGet());
            system.tick();

            assertEquals(1, callbacks.get());

            system.tick();

            assertEquals(1, callbacks.get());
        }
    }

    @Test
    void callbackMutationAffectsNextTick() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            VoltageSource source = VoltageSource.create(system, 5.0);
            Resistance resistor = Resistance.create(system, 10.0);

            connectSourceAndResistor(system, source, resistor);

            ArrayList<ObservationStatus> statuses = new ArrayList<>();
            ArrayList<Double> values = new ArrayList<>();

            system.subscribe(
                resistor, 0, (status, value) -> {
                    statuses.add(status);
                    values.add(value);

                    if (values.size() == 1) {
                        source.setVoltage(7.0);
                    }
                }
            );

            system.tick();

            assertEquals(1, values.size());
            assertEquals(ObservationStatus.AVAILABLE, statuses.getFirst());
            assertEquals(5.0, values.getFirst(), 1e-9);

            system.tick();

            assertEquals(2, values.size());
            assertEquals(ObservationStatus.AVAILABLE, statuses.get(1));
            assertEquals(7.0, values.get(1), 1e-9);
        }
    }

    @Test
    void unsubscribeDuringPublicationPreservesCompletedSnapshot() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger callbacks = new AtomicInteger();

            SubscriptionHolder firstHolder = new SubscriptionHolder();
            SubscriptionHolder secondHolder = new SubscriptionHolder();

            ObservationSubscription first = system.subscribe(
                resistor, 0, (status, value) -> {
                    int invocation = callbacks.incrementAndGet();

                    if (invocation == 1) {
                        secondHolder.get().unsubscribe();
                    }
                }
            );

            firstHolder.set(first);

            ObservationSubscription second = system.subscribe(
                resistor, 1, (status, value) -> {
                    int invocation = callbacks.incrementAndGet();

                    if (invocation == 1) {
                        firstHolder.get().unsubscribe();
                    }
                }
            );

            secondHolder.set(second);

            system.tick();

            /*
             * Whichever native record is published first unsubscribes the
             * other subscription. The second record must nevertheless still
             * be delivered because it belongs to the already-completed tick.
             */
            assertEquals(2, callbacks.get());

            assertTrue(first.isActive() ^ second.isActive(), "Exactly one subscription should have been unsubscribed");

            ObservationSubscription removed = first.isActive() ? second : first;

            assertThrows(IllegalStateException.class, removed::listener);
        }
    }

    @Test
    void subscriptionCreatedDuringPublicationStartsOnNextTick() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger createdCallbacks = new AtomicInteger();
            SubscriptionHolder created = new SubscriptionHolder();

            system.subscribe(
                resistor,
                0,
                (status, value) -> created.set(system.subscribe(
                    resistor,
                    1,
                    (createdStatus, createdValue) -> createdCallbacks.incrementAndGet()
                ))
            );

            system.tick();

            assertTrue(created.get().isActive());
            assertEquals(0, createdCallbacks.get());

            system.tick();

            assertEquals(1, createdCallbacks.get());
        }
    }

    @Test
    void listenerFailureDoesNotAbortPublicationOrPoisonSystem() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            IllegalStateException listenerFailure = new IllegalStateException("listener failure");

            AtomicInteger otherCallbacks = new AtomicInteger();

            system.subscribe(
                resistor, 0, (status, value) -> {
                    throw listenerFailure;
                }
            );

            system.subscribe(resistor, 1, (status, value) -> otherCallbacks.incrementAndGet());

            IllegalStateException thrown = assertThrows(IllegalStateException.class, system::tick);

            assertSame(listenerFailure, thrown);
            assertEquals(1, otherCallbacks.get());

            /*
             * Both observations were already published natively, so an
             * unchanged following tick has no records. More importantly,
             * the callback exception must not have poisoned the system.
             */
            assertDoesNotThrow(system::tick);
            assertDoesNotThrow(system::createWire);
        }
    }

    @Test
    void multipleListenerFailuresAreSuppressed() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            RuntimeException firstFailure = new IllegalStateException("first");

            RuntimeException secondFailure = new IllegalArgumentException("second");

            system.subscribe(
                resistor, 0, (status, value) -> {
                    throw firstFailure;
                }
            );

            system.subscribe(
                resistor, 1, (status, value) -> {
                    throw secondFailure;
                }
            );

            RuntimeException thrown = assertThrows(RuntimeException.class, system::tick);

            assertTrue(thrown == firstFailure || thrown == secondFailure);

            assertEquals(1, thrown.getSuppressed().length);

            RuntimeException otherFailure = thrown == firstFailure ? secondFailure : firstFailure;

            assertSame(otherFailure, thrown.getSuppressed()[0]);

            assertDoesNotThrow(system::tick);
        }
    }

    @Test
    void recursiveTickIsRejectedWithoutAbortingOuterPublication() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger recursiveFailures = new AtomicInteger();
            AtomicInteger otherCallbacks = new AtomicInteger();

            system.subscribe(
                resistor, 0, (status, value) -> {
                    IllegalStateException failure = assertThrows(IllegalStateException.class, system::tick);

                    assertEquals("Recursive calls to tick are not allowed", failure.getMessage());

                    recursiveFailures.incrementAndGet();
                }
            );

            system.subscribe(resistor, 1, (status, value) -> otherCallbacks.incrementAndGet());

            system.tick();

            assertEquals(1, recursiveFailures.get());
            assertEquals(1, otherCallbacks.get());

            // ticking must have been cleared after the outer call.
            assertDoesNotThrow(system::tick);
        }
    }

    @Test
    void closeDuringPublicationIsRejectedWithoutClosingSystem() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger closeFailures = new AtomicInteger();
            AtomicInteger otherCallbacks = new AtomicInteger();

            system.subscribe(
                resistor, 0, (status, value) -> {
                    assertThrows(IllegalStateException.class, system::close);

                    closeFailures.incrementAndGet();
                }
            );

            system.subscribe(resistor, 1, (status, value) -> otherCallbacks.incrementAndGet());

            system.tick();

            assertEquals(1, closeFailures.get());
            assertEquals(1, otherCallbacks.get());

            assertDoesNotThrow(system::createWire);
        }
    }

    @Test
    void deviceRemovalDuringPublicationInvalidatesImmediatelyButPreservesSnapshot() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistor = Resistance.create(system, 10.0);

            AtomicInteger callbacks = new AtomicInteger();
            AtomicBoolean removed = new AtomicBoolean();

            SubscriptionHolder firstHolder = new SubscriptionHolder();
            SubscriptionHolder secondHolder = new SubscriptionHolder();

            ObservationListener listener = (status, value) -> {
                callbacks.incrementAndGet();

                if (removed.compareAndSet(false, true)) {
                    resistor.remove();

                    assertFalse(firstHolder.get().isActive());
                    assertFalse(secondHolder.get().isActive());
                }
            };

            ObservationSubscription first = system.subscribe(resistor, 0, listener);

            firstHolder.set(first);

            ObservationSubscription second = system.subscribe(resistor, 1, listener);

            secondHolder.set(second);

            system.tick();

            assertEquals(2, callbacks.get());

            assertFalse(first.isActive());
            assertFalse(second.isActive());

            assertThrows(IllegalStateException.class, first::listener);
            assertThrows(IllegalStateException.class, second::listener);

            assertDoesNotThrow(system::tick);
        }
    }

    private static void connectSourceAndResistor(ElectricSystem system, VoltageSource source, Resistance resistor) {
        Wire positive = system.createWire();
        Wire negative = system.createWire();

        source.attachPositive(positive);
        source.attachNegative(negative);

        resistor.attachPositive(positive);
        resistor.attachNegative(negative);
    }

    private static final class SubscriptionHolder {
        private @Nullable ObservationSubscription subscription;

        void set(ObservationSubscription subscription) {
            this.subscription = subscription;
        }

        ObservationSubscription get() {
            ObservationSubscription subscription = this.subscription;

            if (subscription == null) {
                throw new IllegalStateException("Subscription has not been initialized");
            }

            return subscription;
        }
    }
}