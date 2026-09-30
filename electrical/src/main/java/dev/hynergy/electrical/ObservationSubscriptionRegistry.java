package dev.hynergy.electrical;

import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import org.jspecify.annotations.Nullable;

import java.util.Objects;

final class ObservationSubscriptionRegistry {
    private final Int2ObjectOpenHashMap<ObservationSubscription> subscriptions = new Int2ObjectOpenHashMap<>();

    private final IntArrayList deferredRemovals = new IntArrayList();

    private boolean publishing;

    void add(ObservationSubscription subscription) {
        Objects.requireNonNull(subscription, "subscription");

        if (!subscription.isActive()) {
            throw new IllegalArgumentException("Cannot register an inactive observation subscription");
        }

        int subscriptionId = subscription.nativeId();

        if (subscriptionId == 0) {
            throw new IllegalArgumentException("Subscription ID must not be zero");
        }

        if (subscriptions.containsKey(subscriptionId)) {
            throw new IllegalStateException(
                "Duplicate observation subscription ID: " + Integer.toUnsignedLong(subscriptionId));
        }

        subscriptions.put(subscriptionId, subscription);
    }

    @SuppressWarnings("DataFlowIssue")
    @Nullable ObservationSubscription get(int subscriptionId) {
        return subscriptions.get(subscriptionId);
    }

    void remove(ObservationSubscription subscription) {
        Objects.requireNonNull(subscription, "subscription");

        int subscriptionId = subscription.nativeId();

        ObservationSubscription registered = subscriptions.get(subscriptionId);

        if (registered != subscription) {
            throw new IllegalStateException("Observation subscription registry ownership mismatch");
        }

        if (!subscription.isActive()) {
            throw new IllegalStateException("Observation subscription is already inactive");
        }

        subscription.deactivate();

        if (publishing) {
            deferredRemovals.add(subscriptionId);
            return;
        }

        subscriptions.remove(subscriptionId);
        subscription.releaseListener();
    }

    void invalidateDevice(int deviceId) {
        var iterator = subscriptions.int2ObjectEntrySet().fastIterator();

        while (iterator.hasNext()) {
            var entry = iterator.next();
            ObservationSubscription subscription = entry.getValue();

            if (subscription.deviceId() != deviceId || !subscription.isActive()) {
                continue;
            }

            subscription.deactivate();

            if (publishing) {
                deferredRemovals.add(entry.getIntKey());
            } else {
                iterator.remove();
                subscription.releaseListener();
            }
        }
    }

    void invalidateAll() {
        for (ObservationSubscription subscription : subscriptions.values()) {
            if (subscription.isActive()) {
                subscription.deactivate();
            }

            subscription.releaseListener();
        }

        subscriptions.clear();
        deferredRemovals.clear();
        publishing = false;
    }

    void beginPublication() {
        if (publishing) {
            throw new IllegalStateException("Observation publication is already in progress");
        }

        if (!deferredRemovals.isEmpty()) {
            throw new IllegalStateException("Deferred observation removals remain from an earlier publication");
        }

        publishing = true;
    }

    void endPublication() {
        if (!publishing) {
            throw new IllegalStateException("Observation publication is not in progress");
        }

        try {
            for (int index = 0; index < deferredRemovals.size(); index++) {
                int subscriptionId = deferredRemovals.getInt(index);

                ObservationSubscription subscription = get(subscriptionId);

                if (subscription == null) {
                    throw new IllegalStateException(
                        "Deferred observation subscription is missing: " + Integer.toUnsignedLong(subscriptionId));
                }

                if (subscription.isActive()) {
                    throw new IllegalStateException(
                        "Deferred observation subscription became active again: " + Integer.toUnsignedLong(
                            subscriptionId));
                }

                subscriptions.remove(subscriptionId);
                subscription.releaseListener();
            }
        } finally {
            deferredRemovals.clear();
            publishing = false;
        }
    }
}