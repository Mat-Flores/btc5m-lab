//! Guards checked before every order (02-conception.md §5), and the network guard every write to a venue goes
//! through. "When in doubt, do not open": any missing condition refuses an opening. Exits stay allowed after the
//! emergency stop and the session loss limit (positions are closed or run to settlement), never when disarmed.
use crate::money::Micro;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    /// Largest amount of one opening order; above, the order is refused, not reduced.
    pub max_order: Micro,
    /// Session loss (realized + latent) at which openings stop, as a positive amount.
    pub session_loss: Micro,
}

/// State the engine knows when an order is about to go.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub sources_valid: bool,
    /// An order whose fate is not known (signed, sent, unknown).
    pub order_pending: bool,
    pub reconciled: bool,
    pub clock_synced: bool,
    /// The market's metadata was read ahead of time.
    pub prepared: bool,
    /// Session result, realized + latent.
    pub session_pnl: Micro,
    /// Monotonic now, for the venue's back-off.
    pub now_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Refusal { NotArmed, Stopped, OverCap { amount: Micro, max: Micro }, SessionLoss { pnl: Micro, limit: Micro }, SourceInvalid, OrderPending, NotReconciled, ClockUnsynced, NotPrepared, BackOff { ms: u64 } }

pub struct Guards {
    pub limits: Limits,
    /// Emergency stop: shared with whoever may pull it; read before every send.
    pub stop: Arc<AtomicBool>,
    /// The venue asked to wait (Retry-After) until this monotonic instant.
    pub back_off_until_ns: u64,
    armed: Arc<AtomicBool>,
}

impl Guards {
    pub fn new(limits: Limits, armed: Arc<AtomicBool>) -> Self { Guards { limits, stop: Arc::new(AtomicBool::new(false)), back_off_until_ns: 0, armed } }

    pub fn check_open(&self, amount: Micro, c: &Context) -> Result<(), Refusal> {
        self.common(c)?;
        if self.stop.load(Ordering::Acquire) { return Err(Refusal::Stopped); }
        if amount > self.limits.max_order { return Err(Refusal::OverCap { amount, max: self.limits.max_order }); }
        if c.session_pnl <= -self.limits.session_loss { return Err(Refusal::SessionLoss { pnl: c.session_pnl, limit: self.limits.session_loss }); }
        if !c.sources_valid { return Err(Refusal::SourceInvalid); }
        Ok(())
    }

    /// Closing a held position: allowed after the stop and the loss limit.
    pub fn check_exit(&self, c: &Context) -> Result<(), Refusal> { self.common(c) }

    fn common(&self, c: &Context) -> Result<(), Refusal> {
        if !self.armed.load(Ordering::Acquire) { return Err(Refusal::NotArmed); }
        if c.now_ns < self.back_off_until_ns { return Err(Refusal::BackOff { ms: (self.back_off_until_ns - c.now_ns) / 1_000_000 }); }
        if c.order_pending { return Err(Refusal::OrderPending); }
        if !c.reconciled { return Err(Refusal::NotReconciled); }
        if !c.clock_synced { return Err(Refusal::ClockUnsynced); }
        if !c.prepared { return Err(Refusal::NotPrepared); }
        Ok(())
    }
}

/// Requests a venue may receive. Everything else is refused, and every write is refused while disarmed.
/// `path` is exact, or a prefix when it ends with '/'.
#[derive(Clone, Copy, Debug)]
pub struct Allowed { pub method: &'static str, pub host: &'static str, pub path: &'static str }

pub struct NetGuard { allowed: &'static [Allowed], armed: Arc<AtomicBool> }

impl NetGuard {
    pub fn new(allowed: &'static [Allowed], armed: Arc<AtomicBool>) -> Self { NetGuard { allowed, armed } }
    /// Reads (GET) pass; a write passes only when armed and listed.
    pub fn permits(&self, method: &str, host: &str, path: &str) -> bool {
        method == "GET" || (self.armed.load(Ordering::Acquire) && self.allowed.iter().any(|a| a.method == method && a.host == host && (path == a.path || (a.path.ends_with('/') && path.starts_with(a.path)))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_ctx() -> Context { Context { sources_valid: true, order_pending: false, reconciled: true, clock_synced: true, prepared: true, session_pnl: 0, now_ns: 10 } }
    fn guards(armed: bool) -> Guards { Guards::new(Limits { max_order: 5_000_000, session_loss: 25_000_000 }, Arc::new(AtomicBool::new(armed))) }

    #[test]
    fn each_condition_refuses_an_opening() {
        let g = guards(true);
        assert_eq!(g.check_open(5_000_000, &ok_ctx()), Ok(()));
        assert_eq!(g.check_open(5_000_001, &ok_ctx()), Err(Refusal::OverCap { amount: 5_000_001, max: 5_000_000 }));
        assert_eq!(g.check_open(1, &Context { session_pnl: -25_000_000, ..ok_ctx() }), Err(Refusal::SessionLoss { pnl: -25_000_000, limit: 25_000_000 }));
        assert_eq!(g.check_open(1, &Context { sources_valid: false, ..ok_ctx() }), Err(Refusal::SourceInvalid));
        assert_eq!(g.check_open(1, &Context { order_pending: true, ..ok_ctx() }), Err(Refusal::OrderPending));
        assert_eq!(g.check_open(1, &Context { reconciled: false, ..ok_ctx() }), Err(Refusal::NotReconciled));
        assert_eq!(g.check_open(1, &Context { clock_synced: false, ..ok_ctx() }), Err(Refusal::ClockUnsynced));
        assert_eq!(g.check_open(1, &Context { prepared: false, ..ok_ctx() }), Err(Refusal::NotPrepared));
        assert_eq!(guards(false).check_open(1, &ok_ctx()), Err(Refusal::NotArmed));
    }

    #[test]
    fn the_stop_blocks_openings_not_exits_and_back_off_blocks_both() {
        let mut g = guards(true);
        g.stop.store(true, Ordering::Release);
        assert_eq!(g.check_open(1, &ok_ctx()), Err(Refusal::Stopped));
        assert_eq!(g.check_exit(&ok_ctx()), Ok(()));
        assert_eq!(g.check_exit(&Context { session_pnl: -100_000_000, ..ok_ctx() }), Ok(()));
        g.back_off_until_ns = 2_000_010;
        assert_eq!(g.check_exit(&ok_ctx()), Err(Refusal::BackOff { ms: 2 }));
        assert_eq!(guards(false).check_exit(&ok_ctx()), Err(Refusal::NotArmed));
    }

    #[test]
    fn net_guard_lets_reads_through_and_listed_writes_only_when_armed() {
        static LIST: [Allowed; 2] = [Allowed { method: "POST", host: "venue.test", path: "/order" }, Allowed { method: "POST", host: "venue.test", path: "/auth/" }];
        let armed = Arc::new(AtomicBool::new(false));
        let n = NetGuard::new(&LIST, armed.clone());
        assert!(n.permits("GET", "venue.test", "/time"));
        assert!(!n.permits("POST", "venue.test", "/order"));
        armed.store(true, Ordering::Release);
        assert!(n.permits("POST", "venue.test", "/order"));
        assert!(!n.permits("POST", "venue.test", "/withdraw"));
        assert!(!n.permits("POST", "venue.test", "/orders"));
        assert!(n.permits("POST", "venue.test", "/auth/api-key"));
        assert!(!n.permits("POST", "other.test", "/order"));
        assert!(!n.permits("DELETE", "venue.test", "/order"));
    }
}
