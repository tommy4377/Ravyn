# Runtime settings
- `max_active` is live through `ConcurrencyGate`: atomic increases/decreases, non-preemptive for active work, and no new admission while usage is above a reduced limit.
- Global HTTP bandwidth and its named-zone schedule remain live.
- API request timeout, concurrency, per-client requests/minute, and burst are shared with middleware and reconfigured live together. Reconfiguration clears token buckets so old refill/burst parameters cannot survive.
- Health request timeout is always capped at five seconds.
- Settings reset reapplies job, bandwidth, and API live baselines; fields not backed by shared mutable runtime state remain `backend_restart`.
- Persisted candidates are validated through `PersistentSettings::apply_to` before saving or applying.