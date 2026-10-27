//! Conformance checks every feed adapter must pass, run from each adapter's own tests on its real messages.
//! Source-level faults (cuts, silences, gaps, duplicates, disorder) are the runner's and supervisor's job and are
//! tested once, in the core.
use crate::clock::Stamp;
use crate::event::Event;
use crate::feed::Adapter;

const RECV: Stamp = Stamp { mono_ns: 1, wall_ns: 1 };

/// Every real frame decodes without error; returns all events, in order.
pub fn decodes(a: &mut dyn Adapter, frames: &[String]) -> Vec<Event> {
    let mut out = Vec::new();
    for f in frames { if let Err(e) = a.decode(f, RECV, 0, &mut out) { panic!("real frame refused: {e:?}\n{f}"); } }
    out
}

/// Every strict prefix of a real frame (a truncated message) is refused, and produces no event.
pub fn truncations_refused(a: &mut dyn Adapter, frame: &str) {
    for (i, _) in frame.char_indices().skip(1) {
        let mut out = Vec::new();
        let r = a.decode(&frame[..i], RECV, 0, &mut out);
        assert!(r.is_err() && out.is_empty(), "truncated frame accepted at byte {i}: {:?}", &frame[..i]);
    }
}

/// Arbitrary text never panics (the caller drives it with generated input).
pub fn never_panics(a: &mut dyn Adapter, text: &str) { let mut out = Vec::new(); let _ = a.decode(text, RECV, 0, &mut out); }
