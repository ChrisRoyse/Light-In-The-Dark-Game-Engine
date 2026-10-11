//! Process-global heap-allocation counter for real per-frame allocation budgeting.
//!
//! Issue #1532: `FrameStats.allocations` was hardcoded `0` at every site, so the
//! allocation term of `FrameBudget::is_within` (`0 <= max_allocations`) could never
//! fail — a false-green that was nonetheless serialized as FSV evidence and asserted
//! `budget_ok == true`. This module promotes the proven `CountingAllocator` pattern
//! from `examples/sim_tick_fsv.rs` / `sim_orders_fsv.rs` into a shared, reusable
//! global allocator so the metric is REAL.
//!
//! Install it once per process:
//! ```ignore
//! #[global_allocator]
//! static GLOBAL: litd_engine::frame_alloc::CountingAllocator =
//!     litd_engine::frame_alloc::CountingAllocator;
//! ```
//! then read [`allocation_count`] at the start and end of a frame; the delta is the
//! number of heap allocations that frame performed.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic count of heap allocations observed by [`CountingAllocator`] since
/// process start. Read with [`allocation_count`]; never reset (frame deltas are
/// computed by the caller subtracting two reads).
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// A `#[global_allocator]` that counts every allocation (and reallocation) and then
/// delegates to the system allocator.
///
/// CRITICAL: the allocator hooks must NEVER themselves allocate — doing so would
/// recurse back into `alloc` and deadlock/overflow. We only touch a lock-free
/// [`AtomicU64`] and forward to `System`, so there is no recursion.
pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Count the reallocation once and delegate to `System::realloc` directly
        // (rather than the default alloc+copy+dealloc, which would double-count).
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Current process-wide allocation count (Relaxed load). Subtract a start-of-frame
/// read from an end-of-frame read to get that frame's real allocation count.
pub fn allocation_count() -> u64 {
    ALLOCATIONS.load(Ordering::Relaxed)
}

/// Real allocations since `start` (a prior [`allocation_count`]), saturating into a
/// `u32` for `FrameStats.allocations`. #1532: this is the per-frame delta the render
/// sites feed into the allocation budget.
pub fn frame_delta(start: u64) -> u32 {
    allocation_count()
        .saturating_sub(start)
        .min(u32::MAX as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A REAL allocation must move the counter. We do not install `CountingAllocator`
    /// as the global allocator in the library test binary (the harness owns that), so
    /// this test drives the counter through the allocator hook directly: read the
    /// count, perform an allocation via the hook, read again, assert the delta.
    #[test]
    fn counter_increments_across_a_real_allocation() {
        let allocator = CountingAllocator;
        let layout = Layout::from_size_align(4096, 8).expect("valid layout");

        let before = allocation_count();
        // SAFETY: `layout` is non-zero and well-formed; we free exactly what we alloc.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(
            !ptr.is_null(),
            "system allocator returned null for 4096 bytes"
        );
        let after = allocation_count();
        unsafe { allocator.dealloc(ptr, layout) };

        assert!(
            after.wrapping_sub(before) >= 1,
            "expected the allocation counter to advance by at least 1 (before={before}, after={after})"
        );
    }
}
