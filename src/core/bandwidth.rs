//! Fair global bandwidth scheduling.
//!
//! The scheduler owns the process-wide download budget and divides it among
//! registered flows (normally one flow per active job) by weighted
//! water-filling:
//!
//! 1. every flow gets a share proportional to its effective weight
//!    (foreground flows count four times a background flow of equal weight);
//! 2. a flow never receives more than its `max_bps` cap — freed budget is
//!    redistributed to the remaining flows;
//! 3. a flow never receives less than its `min_bps` floor (or a small
//!    starvation floor when none is configured), so a heavy foreground flow
//!    cannot silence background flows entirely;
//! 4. if the floors alone exceed the capacity they are scaled down
//!    proportionally.
//!
//! Rates are recomputed whenever a flow registers, unregisters, is
//! reconfigured, or the global capacity changes, so the policy supports live
//! reconfiguration. Between recomputations each flow is limited by its own
//! token bucket; the sum of the buckets never exceeds the global capacity.
//!
//! Not yet implemented (tracked in the master document): time-scheduled
//! profiles and work-conserving redistribution of budget a flow leaves idle
//! between recomputations.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use uuid::Uuid;

use crate::core::rate_limit::RateLimiter;

/// Minimum starvation floor handed to a flow without an explicit `min_bps`,
/// as a divisor of the per-flow equal share. With 8 flows and a 10 MB/s
/// budget each unconfigured flow is still guaranteed ~78 KB/s.
const DEFAULT_FLOOR_DIVISOR: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowClass {
    Foreground,
    Background,
}

impl FlowClass {
    fn weight_multiplier(self) -> u64 {
        match self {
            Self::Foreground => 4,
            Self::Background => 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlowConfig {
    /// Relative weight; zero is treated as one.
    pub weight: u32,
    pub class: FlowClass,
    /// Guaranteed floor in bytes per second.
    pub min_bps: Option<u64>,
    /// Hard cap in bytes per second.
    pub max_bps: Option<u64>,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            weight: 1,
            class: FlowClass::Foreground,
            min_bps: None,
            max_bps: None,
        }
    }
}

struct FlowState {
    config: FlowConfig,
    limiter: Arc<RateLimiter>,
}

#[derive(Clone, Default)]
pub struct FairBandwidthScheduler {
    inner: Arc<Mutex<SchedulerState>>,
}

/// A scoped scheduler registration. Dropping the handle releases the flow and
/// immediately redistributes its bandwidth to the remaining transfers.
pub struct BandwidthFlow {
    scheduler: FairBandwidthScheduler,
    id: Uuid,
    limiter: Arc<RateLimiter>,
}

impl BandwidthFlow {
    pub fn limiter(&self) -> Arc<RateLimiter> {
        self.limiter.clone()
    }
}

impl Drop for BandwidthFlow {
    fn drop(&mut self) {
        self.scheduler.unregister(self.id);
    }
}

#[derive(Default)]
struct SchedulerState {
    /// Zero means unlimited: flows are constrained only by their own caps.
    capacity_bps: u64,
    flows: HashMap<Uuid, FlowState>,
}

impl FairBandwidthScheduler {
    pub fn new(capacity_bps: u64) -> Self {
        let scheduler = Self::default();
        scheduler.set_capacity_bps(capacity_bps);
        scheduler
    }

    /// Updates the global budget and immediately re-divides it.
    pub fn set_capacity_bps(&self, capacity_bps: u64) {
        let mut state = self.lock();
        state.capacity_bps = capacity_bps;
        recompute(&mut state);
    }

    pub fn capacity_bps(&self) -> u64 {
        self.lock().capacity_bps
    }

    /// Registers a flow and returns the limiter it must consume from. The
    /// limiter stays valid after unregistration (it simply keeps the last
    /// assigned rate), so a finishing transfer never blocks on a gone flow.
    pub fn register(&self, id: Uuid, config: FlowConfig) -> Arc<RateLimiter> {
        let limiter = Arc::new(RateLimiter::new(0));
        let mut state = self.lock();
        state.flows.insert(
            id,
            FlowState {
                config,
                limiter: limiter.clone(),
            },
        );
        recompute(&mut state);
        limiter
    }

    pub fn register_scoped(&self, id: Uuid, config: FlowConfig) -> BandwidthFlow {
        BandwidthFlow {
            scheduler: self.clone(),
            id,
            limiter: self.register(id, config),
        }
    }

    /// Reconfigures a live flow; unknown ids are ignored.
    pub fn reconfigure(&self, id: Uuid, config: FlowConfig) {
        let mut state = self.lock();
        if let Some(flow) = state.flows.get_mut(&id) {
            flow.config = config;
            recompute(&mut state);
        }
    }

    pub fn unregister(&self, id: Uuid) {
        let mut state = self.lock();
        if state.flows.remove(&id).is_some() {
            recompute(&mut state);
        }
    }

    pub fn flow_count(&self) -> usize {
        self.lock().flows.len()
    }

    /// The rate currently assigned to a flow, for diagnostics and tests.
    pub fn assigned_bps(&self, id: Uuid) -> Option<u64> {
        self.lock()
            .flows
            .get(&id)
            .map(|flow| flow.limiter.bytes_per_second())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SchedulerState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn recompute(state: &mut SchedulerState) {
    let capacity = state.capacity_bps;
    if state.flows.is_empty() {
        return;
    }
    if capacity == 0 {
        for flow in state.flows.values() {
            flow.limiter
                .set_bytes_per_second(flow.config.max_bps.unwrap_or(0));
        }
        return;
    }

    let flow_count = state.flows.len() as u64;
    let default_floor = (capacity / (flow_count * DEFAULT_FLOOR_DIVISOR)).max(1);

    struct Entry {
        id: Uuid,
        weight: u64,
        floor: u64,
        cap: u64,
    }
    let mut entries: Vec<Entry> = state
        .flows
        .iter()
        .map(|(id, flow)| {
            let cap = flow.config.max_bps.unwrap_or(u64::MAX).max(1);
            let floor = flow.config.min_bps.unwrap_or(default_floor).min(cap);
            Entry {
                id: *id,
                weight: u64::from(flow.config.weight.max(1))
                    * flow.config.class.weight_multiplier(),
                floor,
                cap,
            }
        })
        .collect();

    // If the guaranteed floors alone exceed the budget, scale them down
    // proportionally so the sum of assignments never exceeds the capacity.
    let floor_sum: u128 = entries.iter().map(|entry| u128::from(entry.floor)).sum();
    if floor_sum > u128::from(capacity) {
        for entry in &mut entries {
            let scaled = u128::from(entry.floor) * u128::from(capacity) / floor_sum;
            entry.floor = u64::try_from(scaled).unwrap_or(u64::MAX);
        }
    }

    // Weighted water-filling: repeatedly hand every unresolved flow its
    // weight-proportional share of the remaining budget, pinning flows that
    // hit their cap or floor, until the assignment is stable.
    let mut assigned: HashMap<Uuid, u64> = HashMap::new();
    let mut remaining = capacity;
    let mut open: Vec<usize> = (0..entries.len()).collect();
    while !open.is_empty() {
        let weight_sum: u128 = open
            .iter()
            .map(|index| u128::from(entries[*index].weight))
            .sum();
        let mut pinned = Vec::new();
        for (position, index) in open.iter().copied().enumerate() {
            let entry = &entries[index];
            let share =
                u64::try_from(u128::from(remaining) * u128::from(entry.weight) / weight_sum.max(1))
                    .unwrap_or(u64::MAX);
            if share >= entry.cap {
                assigned.insert(entry.id, entry.cap);
                pinned.push(position);
            } else if share < entry.floor {
                assigned.insert(entry.id, entry.floor);
                pinned.push(position);
            }
        }
        if pinned.is_empty() {
            for index in &open {
                let entry = &entries[*index];
                let share = u64::try_from(
                    u128::from(remaining) * u128::from(entry.weight) / weight_sum.max(1),
                )
                .unwrap_or(u64::MAX);
                assigned.insert(entry.id, share.clamp(entry.floor, entry.cap));
            }
            break;
        }
        for position in pinned.into_iter().rev() {
            let index = open.swap_remove(position);
            remaining = remaining.saturating_sub(assigned[&entries[index].id]);
        }
    }

    for (id, flow) in &state.flows {
        let rate = assigned.get(id).copied().unwrap_or(default_floor);
        flow.limiter.set_bytes_per_second(rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPACITY: u64 = 1_000_000;

    fn flow(weight: u32, class: FlowClass) -> FlowConfig {
        FlowConfig {
            weight,
            class,
            min_bps: None,
            max_bps: None,
        }
    }

    #[test]
    fn shares_follow_weights() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        scheduler.register(a, flow(1, FlowClass::Foreground));
        scheduler.register(b, flow(3, FlowClass::Foreground));
        assert_eq!(scheduler.assigned_bps(a), Some(CAPACITY / 4));
        assert_eq!(scheduler.assigned_bps(b), Some(CAPACITY / 4 * 3));
    }

    #[test]
    fn background_flows_yield_to_foreground_but_are_not_starved() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let fg = Uuid::new_v4();
        let bg = Uuid::new_v4();
        scheduler.register(fg, flow(1, FlowClass::Foreground));
        scheduler.register(bg, flow(1, FlowClass::Background));
        let foreground = scheduler.assigned_bps(fg).unwrap();
        let background = scheduler.assigned_bps(bg).unwrap();
        assert_eq!(foreground, CAPACITY / 5 * 4);
        assert_eq!(background, CAPACITY / 5);
        assert!(background > 0);
    }

    #[test]
    fn caps_are_respected_and_headroom_is_redistributed() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let capped = Uuid::new_v4();
        let open = Uuid::new_v4();
        scheduler.register(
            capped,
            FlowConfig {
                max_bps: Some(100_000),
                ..flow(1, FlowClass::Foreground)
            },
        );
        scheduler.register(open, flow(1, FlowClass::Foreground));
        assert_eq!(scheduler.assigned_bps(capped), Some(100_000));
        assert_eq!(scheduler.assigned_bps(open), Some(CAPACITY - 100_000));
    }

    #[test]
    fn floors_are_respected_against_heavy_competition() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let heavy = Uuid::new_v4();
        let floored = Uuid::new_v4();
        scheduler.register(heavy, flow(1_000, FlowClass::Foreground));
        scheduler.register(
            floored,
            FlowConfig {
                min_bps: Some(200_000),
                ..flow(1, FlowClass::Background)
            },
        );
        assert_eq!(scheduler.assigned_bps(floored), Some(200_000));
        let heavy_rate = scheduler.assigned_bps(heavy).unwrap();
        assert!(heavy_rate <= CAPACITY - 200_000);
    }

    #[test]
    fn overcommitted_floors_are_scaled_within_capacity() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            scheduler.register(
                *id,
                FlowConfig {
                    min_bps: Some(CAPACITY),
                    ..flow(1, FlowClass::Foreground)
                },
            );
        }
        let total: u64 = ids
            .iter()
            .map(|id| scheduler.assigned_bps(*id).unwrap())
            .sum();
        assert!(total <= CAPACITY, "assignments exceed capacity: {total}");
    }

    #[test]
    fn capacity_smaller_than_flow_count_never_overcommits() {
        let scheduler = FairBandwidthScheduler::new(1);
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            scheduler.register(*id, flow(1, FlowClass::Foreground));
        }
        let total: u64 = ids
            .iter()
            .map(|id| scheduler.assigned_bps(*id).unwrap())
            .sum();
        assert!(total <= 1, "assignments exceed tiny capacity: {total}");
    }

    #[test]
    fn assignments_never_exceed_capacity() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let ids: Vec<Uuid> = (0..7).map(|_| Uuid::new_v4()).collect();
        for (index, id) in ids.iter().enumerate() {
            scheduler.register(
                *id,
                FlowConfig {
                    weight: index as u32 + 1,
                    class: if index % 2 == 0 {
                        FlowClass::Foreground
                    } else {
                        FlowClass::Background
                    },
                    min_bps: (index % 3 == 0).then_some(50_000),
                    max_bps: (index % 4 == 0).then_some(120_000),
                },
            );
        }
        let total: u64 = ids
            .iter()
            .map(|id| scheduler.assigned_bps(*id).unwrap())
            .sum();
        assert!(total <= CAPACITY, "assignments exceed capacity: {total}");
    }

    #[test]
    fn unlimited_capacity_applies_only_explicit_caps() {
        let scheduler = FairBandwidthScheduler::new(0);
        let capped = Uuid::new_v4();
        let open = Uuid::new_v4();
        scheduler.register(
            capped,
            FlowConfig {
                max_bps: Some(250_000),
                ..flow(1, FlowClass::Foreground)
            },
        );
        scheduler.register(open, flow(1, FlowClass::Foreground));
        assert_eq!(scheduler.assigned_bps(capped), Some(250_000));
        assert_eq!(scheduler.assigned_bps(open), Some(0));
    }

    #[test]
    fn live_reconfiguration_updates_assignments() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        scheduler.register(a, flow(1, FlowClass::Foreground));
        scheduler.register(b, flow(1, FlowClass::Foreground));
        assert_eq!(scheduler.assigned_bps(a), Some(CAPACITY / 2));

        scheduler.set_capacity_bps(CAPACITY / 2);
        assert_eq!(scheduler.assigned_bps(a), Some(CAPACITY / 4));

        scheduler.reconfigure(a, flow(3, FlowClass::Foreground));
        assert_eq!(scheduler.assigned_bps(a), Some(CAPACITY / 2 / 4 * 3));

        scheduler.unregister(b);
        assert_eq!(scheduler.assigned_bps(a), Some(CAPACITY / 2));
        assert_eq!(scheduler.flow_count(), 1);
    }

    #[test]
    fn scoped_registration_releases_and_redistributes_capacity() {
        let scheduler = FairBandwidthScheduler::new(CAPACITY);
        let persistent = Uuid::new_v4();
        scheduler.register(persistent, flow(1, FlowClass::Foreground));
        {
            let temporary =
                scheduler.register_scoped(Uuid::new_v4(), flow(1, FlowClass::Foreground));
            assert_eq!(temporary.limiter().bytes_per_second(), CAPACITY / 2);
            assert_eq!(scheduler.assigned_bps(persistent), Some(CAPACITY / 2));
        }
        assert_eq!(scheduler.flow_count(), 1);
        assert_eq!(scheduler.assigned_bps(persistent), Some(CAPACITY));
    }
}
