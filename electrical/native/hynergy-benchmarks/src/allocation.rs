use std::alloc::{GlobalAlloc, Layout};
use std::ops::AddAssign;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AllocationStats {
    allocation_calls: u64,
    deallocation_calls: u64,
    reallocation_calls: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
}

impl AllocationStats {
    #[inline]
    pub const fn allocation_calls(self) -> u64 {
        self.allocation_calls
    }

    #[inline]
    pub const fn deallocation_calls(self) -> u64 {
        self.deallocation_calls
    }

    #[inline]
    pub const fn reallocation_calls(self) -> u64 {
        self.reallocation_calls
    }

    #[inline]
    pub const fn allocated_bytes(self) -> u64 {
        self.allocated_bytes
    }

    #[inline]
    pub const fn deallocated_bytes(self) -> u64 {
        self.deallocated_bytes
    }
}

impl AddAssign for AllocationStats {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.allocation_calls += rhs.allocation_calls;
        self.deallocation_calls += rhs.deallocation_calls;
        self.reallocation_calls += rhs.reallocation_calls;
        self.allocated_bytes += rhs.allocated_bytes;
        self.deallocated_bytes += rhs.deallocated_bytes;
    }
}

pub struct CountingAllocator<A> {
    inner: A,
    enabled: AtomicBool,
    allocation_calls: AtomicU64,
    deallocation_calls: AtomicU64,
    reallocation_calls: AtomicU64,
    allocated_bytes: AtomicU64,
    deallocated_bytes: AtomicU64,
}

impl<A> CountingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
            enabled: AtomicBool::new(false),
            allocation_calls: AtomicU64::new(0),
            deallocation_calls: AtomicU64::new(0),
            reallocation_calls: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            deallocated_bytes: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn start(&self) {
        debug_assert!(
            !self.enabled.load(Ordering::Relaxed),
            "allocation measurement must not be nested",
        );

        self.allocation_calls.store(0, Ordering::Relaxed);
        self.deallocation_calls.store(0, Ordering::Relaxed);
        self.reallocation_calls.store(0, Ordering::Relaxed);
        self.allocated_bytes.store(0, Ordering::Relaxed);
        self.deallocated_bytes.store(0, Ordering::Relaxed);

        self.enabled.store(true, Ordering::SeqCst);
    }

    #[inline]
    pub fn stop(&self) -> AllocationStats {
        self.enabled.store(false, Ordering::SeqCst);

        AllocationStats {
            allocation_calls: self.allocation_calls.load(Ordering::Relaxed),
            deallocation_calls: self.deallocation_calls.load(Ordering::Relaxed),
            reallocation_calls: self.reallocation_calls.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.load(Ordering::Relaxed),
        }
    }

    #[inline]
    fn tracking(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    #[inline]
    fn record_allocation(&self, bytes: usize) {
        self.allocation_calls.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    #[inline]
    fn record_deallocation(&self, bytes: usize) {
        self.deallocation_calls.fetch_add(1, Ordering::Relaxed);
        self.deallocated_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    #[inline]
    fn record_reallocation(&self, old_bytes: usize, new_bytes: usize) {
        self.reallocation_calls.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes
            .fetch_add(new_bytes as u64, Ordering::Relaxed);
        self.deallocated_bytes
            .fetch_add(old_bytes as u64, Ordering::Relaxed);
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc(layout) };

        if !ptr.is_null() && self.tracking() {
            self.record_allocation(layout.size());
        }

        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc_zeroed(layout) };

        if !ptr.is_null() && self.tracking() {
            self.record_allocation(layout.size());
        }

        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.tracking() {
            self.record_deallocation(layout.size());
        }

        unsafe { self.inner.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };

        if !new_ptr.is_null() && self.tracking() {
            self.record_reallocation(layout.size(), new_size);
        }

        new_ptr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::System;

    #[test]
    fn counts_manual_allocation_and_deallocation_only_while_enabled() {
        let allocator = CountingAllocator::new(System);
        let layout = Layout::from_size_align(64, 8).unwrap();

        let ignored = unsafe { allocator.alloc(layout) };
        assert!(!ignored.is_null());
        unsafe { allocator.dealloc(ignored, layout) };

        allocator.start();

        let tracked = unsafe { allocator.alloc(layout) };
        assert!(!tracked.is_null());
        unsafe { allocator.dealloc(tracked, layout) };

        let stats = allocator.stop();

        assert_eq!(stats.allocation_calls(), 1);
        assert_eq!(stats.deallocation_calls(), 1);
        assert_eq!(stats.reallocation_calls(), 0);
        assert_eq!(stats.allocated_bytes(), 64);
        assert_eq!(stats.deallocated_bytes(), 64);
    }

    #[test]
    fn successful_reallocation_is_reported_as_byte_churn() {
        let allocator = CountingAllocator::new(System);
        let layout = Layout::from_size_align(32, 8).unwrap();

        allocator.start();

        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());

        let ptr = unsafe { allocator.realloc(ptr, layout, 96) };
        assert!(!ptr.is_null());

        let new_layout = Layout::from_size_align(96, 8).unwrap();
        unsafe { allocator.dealloc(ptr, new_layout) };

        let stats = allocator.stop();

        assert_eq!(stats.allocation_calls(), 1);
        assert_eq!(stats.reallocation_calls(), 1);
        assert_eq!(stats.deallocation_calls(), 1);
        assert_eq!(stats.allocated_bytes(), 32 + 96);
        assert_eq!(stats.deallocated_bytes(), 32 + 96);
    }

    #[test]
    fn start_resets_previous_measurement() {
        let allocator = CountingAllocator::new(System);
        let layout = Layout::from_size_align(16, 8).unwrap();

        allocator.start();
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());
        unsafe { allocator.dealloc(ptr, layout) };
        let first = allocator.stop();

        assert_eq!(first.allocation_calls(), 1);

        allocator.start();
        let second = allocator.stop();

        assert_eq!(second, AllocationStats::default());
    }
}
