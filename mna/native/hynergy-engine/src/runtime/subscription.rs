use crate::compile::island::DeviceObserver;
use crate::topology::DeviceComponent;
use hynergy_ids::define_non_zero_id;
use hynergy_model::device::definition::{DefinitionObserverId, DeviceId, DevicePartitionId};
use thiserror::Error;

define_non_zero_id!(SubscriptionId);

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("world does not exist")]
    UnknownWorld,

    #[error("device {device:?} does not exist")]
    UnknownDevice { device: DeviceId },

    #[error("device {device:?} has no observer {observer:?}")]
    UnknownObserver {
        device: DeviceId,
        observer: DefinitionObserverId,
    },

    #[error("subscription ID space is exhausted")]
    IdExhausted,

    #[error("subscription {subscription:?} does not exist")]
    UnknownSubscription { subscription: SubscriptionId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObserverSubscription {
    id: SubscriptionId,
    observer: DeviceObserver,
    partition: DevicePartitionId,
    published_bits: Option<u64>,
}

impl ObserverSubscription {
    #[inline]
    const fn new(
        id: SubscriptionId,
        observer: DeviceObserver,
        partition: DevicePartitionId,
    ) -> Self {
        Self {
            id,
            observer,
            partition,
            published_bits: None,
        }
    }

    #[inline]
    pub(crate) const fn id(self) -> SubscriptionId {
        self.id
    }

    #[inline]
    pub(crate) const fn observer(self) -> DeviceObserver {
        self.observer
    }

    #[inline]
    pub(crate) const fn component(self) -> DeviceComponent {
        DeviceComponent::new(self.observer.device(), self.partition)
    }

    #[inline]
    pub(crate) const fn published_bits(self) -> Option<u64> {
        self.published_bits
    }

    #[inline]
    pub(crate) fn set_published_bits(&mut self, bits: u64) {
        self.published_bits = Some(bits);
    }
}

#[derive(Debug)]
pub(crate) struct SubscriptionRegistry {
    next_id: Option<u32>,
    subscriptions: Vec<ObserverSubscription>,
}

impl Default for SubscriptionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionRegistry {
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            next_id: Some(1),
            subscriptions: Vec::new(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        observer: DeviceObserver,
        partition: DevicePartitionId,
    ) -> Option<SubscriptionId> {
        let raw = self.next_id?;

        let id = SubscriptionId::try_from(raw).expect("subscription IDs start at one");

        self.next_id = raw.checked_add(1);

        self.subscriptions
            .push(ObserverSubscription::new(id, observer, partition));

        Some(id)
    }

    pub(crate) fn remove(&mut self, id: SubscriptionId) -> bool {
        let Ok(index) = self
            .subscriptions
            .binary_search_by_key(&id, |subscription| subscription.id())
        else {
            return false;
        };

        self.subscriptions.remove(index);

        true
    }

    pub(crate) fn remove_device(&mut self, device: DeviceId) {
        self.subscriptions
            .retain(|subscription| subscription.observer().device() != device);
    }

    #[inline]
    pub(crate) fn subscriptions(&self) -> &[ObserverSubscription] {
        &self.subscriptions
    }

    #[inline]
    pub(crate) fn subscriptions_mut(&mut self) -> &mut [ObserverSubscription] {
        &mut self.subscriptions
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubscriptionUpdate {
    subscription: SubscriptionId,
    value: f64,
}

impl SubscriptionUpdate {
    #[inline]
    pub(crate) const fn new(subscription: SubscriptionId, value: f64) -> Self {
        Self {
            subscription,
            value,
        }
    }

    #[inline]
    pub const fn subscription(self) -> SubscriptionId {
        self.subscription
    }

    #[inline]
    pub const fn value(self) -> f64 {
        self.value
    }
}
