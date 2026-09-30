package dev.hynergy.electrical;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class ObservationSubscriptionRegistryTest {
    private static final ObservationListener NOOP_LISTENER = (status, value) -> {
    };

    @Test
    void addedSubscriptionIsAvailableById() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationSubscription subscription = subscription(context, 7, 1);

            registry.add(subscription);

            assertSame(subscription, registry.get(7));
            assertTrue(subscription.isActive());
        }
    }

    @Test
    void missingSubscriptionReturnsNull() {
        ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

        assertNull(registry.get(123));
    }

    @Test
    void duplicateSubscriptionIdIsRejectedWithoutReplacingExisting() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationSubscription first = subscription(context, 7, 1);
            ObservationSubscription second = subscription(context, 7, 2);

            registry.add(first);

            assertThrows(IllegalStateException.class, () -> registry.add(second));

            assertSame(first, registry.get(7));
            assertTrue(first.isActive());
            assertTrue(second.isActive());
        }
    }

    @Test
    void negativeSubscriptionIdIsValid() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();
            ObservationSubscription subscription = subscription(context, Integer.MIN_VALUE, 1);

            registry.add(subscription);

            assertSame(subscription, registry.get(Integer.MIN_VALUE));
        }
    }

    @Test
    void zeroSubscriptionIdIsRejected() {
        try (TestContext context = new TestContext()) {
            assertThrows(
                IllegalArgumentException.class,
                () -> new ObservationSubscription(context.system, 0, 1, NOOP_LISTENER)
            );
        }
    }

    @Test
    void invalidDeviceIdIsRejected() {
        try (TestContext context = new TestContext()) {
            assertThrows(
                IllegalArgumentException.class,
                () -> new ObservationSubscription(context.system, 1, 0, NOOP_LISTENER)
            );

            assertThrows(
                IllegalArgumentException.class,
                () -> new ObservationSubscription(context.system, 1, -1, NOOP_LISTENER)
            );
        }
    }

    @Test
    void inactiveSubscriptionCannotBeAdded() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();
            ObservationSubscription subscription = subscription(context, 7, 1);

            subscription.deactivate();

            assertThrows(IllegalArgumentException.class, () -> registry.add(subscription));

            assertNull(registry.get(7));
        }
    }

    @Test
    void normalRemovalImmediatelyRemovesAndReleasesSubscription() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationListener listener = (status, value) -> {
            };

            ObservationSubscription subscription = new ObservationSubscription(context.system, 7, 1, listener);

            registry.add(subscription);
            registry.remove(subscription);

            assertFalse(subscription.isActive());
            assertNull(registry.get(7));

            assertThrows(IllegalStateException.class, subscription::listener);
        }
    }

    @Test
    void removalDuringPublicationPreservesSnapshotLookupAndListener() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationListener listener = (status, value) -> {
            };

            ObservationSubscription subscription = new ObservationSubscription(context.system, 7, 1, listener);

            registry.add(subscription);

            registry.beginPublication();
            registry.remove(subscription);

            assertFalse(subscription.isActive());

            assertSame(subscription, registry.get(7));
            assertSame(listener, subscription.listener());

            registry.endPublication();

            assertNull(registry.get(7));

            assertThrows(IllegalStateException.class, subscription::listener);
        }
    }

    @Test
    void invalidateDeviceImmediatelyRemovesOnlyMatchingSubscriptions() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationSubscription first = subscription(context, 10, 1);
            ObservationSubscription second = subscription(context, 11, 1);
            ObservationSubscription other = subscription(context, 12, 2);

            registry.add(first);
            registry.add(second);
            registry.add(other);

            registry.invalidateDevice(1);

            assertFalse(first.isActive());
            assertFalse(second.isActive());
            assertTrue(other.isActive());

            assertNull(registry.get(10));
            assertNull(registry.get(11));
            assertSame(other, registry.get(12));

            assertThrows(IllegalStateException.class, first::listener);
            assertThrows(IllegalStateException.class, second::listener);

            assertSame(NOOP_LISTENER, other.listener());
        }
    }

    @Test
    void invalidateDeviceDuringPublicationDefersPhysicalRemoval() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationSubscription first = subscription(context, 10, 1);
            ObservationSubscription second = subscription(context, 11, 1);
            ObservationSubscription other = subscription(context, 12, 2);

            registry.add(first);
            registry.add(second);
            registry.add(other);

            registry.beginPublication();
            registry.invalidateDevice(1);

            assertFalse(first.isActive());
            assertFalse(second.isActive());
            assertTrue(other.isActive());

            assertSame(first, registry.get(10));
            assertSame(second, registry.get(11));
            assertSame(other, registry.get(12));

            assertSame(NOOP_LISTENER, first.listener());
            assertSame(NOOP_LISTENER, second.listener());

            registry.endPublication();

            assertNull(registry.get(10));
            assertNull(registry.get(11));
            assertSame(other, registry.get(12));

            assertThrows(IllegalStateException.class, first::listener);

            assertThrows(IllegalStateException.class, second::listener);
        }
    }

    @Test
    void subscriptionAddedDuringPublicationSurvivesDeferredCleanup() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

            ObservationSubscription removed = subscription(context, 10, 1);

            registry.add(removed);

            registry.beginPublication();
            registry.remove(removed);

            ObservationSubscription added = subscription(context, 11, 1);

            registry.add(added);
            registry.endPublication();

            assertNull(registry.get(10));

            assertSame(added, registry.get(11));

            assertTrue(added.isActive());
            assertSame(NOOP_LISTENER, added.listener());
        }
    }

    @Test
    void invalidateAllClearsActiveAndDeferredSubscriptions() {
        try (TestContext context = new TestContext()) {
            ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();
            ObservationSubscription deferred = subscription(context, 10, 1);
            ObservationSubscription active = subscription(context, 11, 2);

            registry.add(deferred);
            registry.add(active);

            registry.beginPublication();
            registry.remove(deferred);

            registry.invalidateAll();

            assertFalse(deferred.isActive());
            assertFalse(active.isActive());

            assertNull(registry.get(10));
            assertNull(registry.get(11));

            assertThrows(IllegalStateException.class, deferred::listener);

            assertThrows(IllegalStateException.class, active::listener);

            assertDoesNotThrow(registry::beginPublication);
            assertDoesNotThrow(registry::endPublication);
        }
    }

    @Test
    void publicationLifecycleRejectsInvalidTransitions() {
        ObservationSubscriptionRegistry registry = new ObservationSubscriptionRegistry();

        assertThrows(IllegalStateException.class, registry::endPublication);

        registry.beginPublication();

        assertThrows(IllegalStateException.class, registry::beginPublication);

        assertDoesNotThrow(registry::endPublication);
    }

    private static ObservationSubscription subscription(TestContext context, int nativeId, int deviceId) {
        return new ObservationSubscription(context.system, nativeId, deviceId, NOOP_LISTENER);
    }

    private static final class TestContext implements AutoCloseable {

        private final ElectricalRuntime runtime = ElectricalRuntime.create();
        private final ElectricSystem system = runtime.createSystem(20);

        @Override
        public void close() {
            system.close();
            runtime.close();
        }
    }
}
