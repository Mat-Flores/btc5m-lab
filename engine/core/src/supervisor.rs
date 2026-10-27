//! Health of one source, as a pure state machine: the core runs one per source, whatever the source.
//! Silence, sequence gaps and disconnections make a source invalid; it is valid again on the first message of a
//! fresh connection. Duplicates (seq already seen) are dropped, never applied twice.
use crate::event::Caps;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Reason { Connecting, Silence { ms: u64 }, SeqGap { expected: u64, got: u64 }, Disconnected, Decode }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Health { Valid, Invalid(Reason) }

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Apply the message.
    Accept,
    /// Already seen (or older): drop it.
    Duplicate,
    /// A gap: drop it, and resynchronise (reconnect).
    Resync,
}

#[derive(Debug)]
pub struct Supervisor {
    caps: Caps,
    pub health: Health,
    last_msg_ns: Option<u64>,
    next_seq: Option<u64>,
    pub reconnects: u64,
    pub duplicates: u64,
    pub gaps: u64,
}

impl Supervisor {
    pub fn new(caps: Caps) -> Self { Supervisor { caps, health: Health::Invalid(Reason::Connecting), last_msg_ns: None, next_seq: None, reconnects: 0, duplicates: 0, gaps: 0 } }

    /// A new connection is open: the sequence starts over (sources reset it per connection).
    pub fn on_connect(&mut self, now_ns: u64) {
        self.next_seq = None; self.last_msg_ns = Some(now_ns);
        // a gap condemns its connection only: the new one starts clean
        if matches!(self.health, Health::Invalid(Reason::SeqGap { .. })) { self.health = Health::Invalid(Reason::Connecting); }
    }

    pub fn on_disconnect(&mut self) -> Option<Health> { self.reconnects += 1; self.set(Health::Invalid(Reason::Disconnected)) }

    /// Every message, before its content is used. Returns what to do with it and the health change, if any.
    pub fn on_message(&mut self, now_ns: u64, seq: Option<u64>) -> (Verdict, Option<Health>) {
        self.last_msg_ns = Some(now_ns);
        // after a gap, nothing on this connection can be trusted until it is replaced
        if matches!(self.health, Health::Invalid(Reason::SeqGap { .. })) { return (Verdict::Resync, None); }
        if self.caps.sequence {
            if let Some(s) = seq {
                match self.next_seq {
                    Some(n) if s < n => { self.duplicates += 1; return (Verdict::Duplicate, None); }
                    Some(n) if s > n => { self.gaps += 1; let h = self.set(Health::Invalid(Reason::SeqGap { expected: n, got: s })); return (Verdict::Resync, h); }
                    _ => self.next_seq = Some(s + 1),
                }
            }
        }
        (Verdict::Accept, self.set(Health::Valid))
    }

    /// Called on a timer: a silent source becomes invalid (and should be reconnected by the caller).
    pub fn on_timer(&mut self, now_ns: u64) -> Option<Health> {
        let last = self.last_msg_ns?;
        let ms = now_ns.saturating_sub(last) / 1_000_000;
        if ms >= self.caps.silence.as_millis() as u64 && !matches!(self.health, Health::Invalid(Reason::Silence { .. })) {
            return self.set(Health::Invalid(Reason::Silence { ms }));
        }
        None
    }

    pub fn on_decode_error(&mut self) -> Option<Health> { self.set(Health::Invalid(Reason::Decode)) }

    pub fn silent_for(&self, now_ns: u64) -> u64 { self.last_msg_ns.map_or(0, |l| now_ns.saturating_sub(l) / 1_000_000) }

    fn set(&mut self, h: Health) -> Option<Health> { if self.health != h { self.health = h.clone(); Some(h) } else { None } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::BookKind;
    use std::time::Duration;

    fn caps(sequence: bool) -> Caps { Caps { sequence, event_time: true, book: BookKind::None, silence: Duration::from_millis(2000) } }
    const MS: u64 = 1_000_000;

    #[test]
    fn valid_on_first_message_invalid_on_silence_and_back() {
        let mut s = Supervisor::new(caps(false));
        s.on_connect(0);
        assert_eq!(s.on_message(10 * MS, None), (Verdict::Accept, Some(Health::Valid)));
        assert_eq!(s.on_timer(1500 * MS), None);
        assert_eq!(s.on_timer(2010 * MS), Some(Health::Invalid(Reason::Silence { ms: 2000 })));
        assert_eq!(s.on_timer(3000 * MS), None);
        assert_eq!(s.on_message(3001 * MS, None).1, Some(Health::Valid));
    }

    #[test]
    fn duplicates_dropped_gaps_resync_until_reconnect() {
        let mut s = Supervisor::new(caps(true));
        s.on_connect(0);
        assert_eq!(s.on_message(1, Some(1)).0, Verdict::Accept);
        assert_eq!(s.on_message(2, Some(2)).0, Verdict::Accept);
        assert_eq!(s.on_message(3, Some(2)).0, Verdict::Duplicate);
        assert_eq!(s.on_message(4, Some(1)).0, Verdict::Duplicate);
        let (v, h) = s.on_message(5, Some(5));
        assert_eq!(v, Verdict::Resync);
        assert_eq!(h, Some(Health::Invalid(Reason::SeqGap { expected: 3, got: 5 })));
        // stays invalid on the same connection, whatever arrives
        assert_eq!(s.on_message(6, Some(6)).0, Verdict::Resync);
        assert_eq!(s.on_disconnect(), Some(Health::Invalid(Reason::Disconnected)));
        s.on_connect(7);
        assert_eq!(s.on_message(8, Some(1)), (Verdict::Accept, Some(Health::Valid)));
        assert_eq!((s.duplicates, s.gaps, s.reconnects), (2, 1, 1));
    }

    #[test]
    fn a_gap_does_not_survive_the_reconnection() {
        let mut s = Supervisor::new(caps(true));
        s.on_connect(0);
        s.on_message(1, Some(1));
        assert_eq!(s.on_message(2, Some(3)).0, Verdict::Resync);
        s.on_connect(3); // the runner reconnects without a disconnect notice
        assert_eq!(s.on_message(4, Some(1)), (Verdict::Accept, Some(Health::Valid)));
    }

    #[test]
    fn unsequenced_source_ignores_seq() {
        let mut s = Supervisor::new(caps(false));
        s.on_connect(0);
        assert_eq!(s.on_message(1, Some(9)).0, Verdict::Accept);
        assert_eq!(s.on_message(2, Some(3)).0, Verdict::Accept);
    }
}
