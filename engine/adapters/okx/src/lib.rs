//! OKX public `bbo-tbt`: top of book pushed every 10 ms when it changes, stamped by the matching engine (`ts`).
//! Topics are instrument ids ("BTC-USDT-SWAP"), one per source line: a quote carries no instrument.
//! `seqId` is the book's sequence, not the channel's: it jumps between frames (no `prevSeqId`), so losses cannot be
//! detected on this channel and the source declares no sequence.
use engine_core::clock::Stamp;
use engine_core::event::{BookKind, Caps, Event, Payload, SourceId};
use engine_core::feed::{Adapter, DecodeError};
use engine_core::money::parse_micro;
use serde::Deserialize;
use std::time::Duration;

pub const URL: &str = "wss://ws.okx.com:8443/ws/v5/public";
/// Calibrated on 40 min of live data (phase 1 and 5): max interval 1.46 s on the swap, 2.57 s on spot.
/// shortcut: one threshold for both instruments, tight for spot; recalibrate on the 24 h run (phase 9).
pub const SILENCE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct BboTbt { inst: Option<String> }

#[derive(Deserialize)]
struct Arg<'a> { channel: &'a str, #[serde(rename = "instId")] inst_id: &'a str }
#[derive(Deserialize)]
struct D<'a> { #[serde(borrow)] asks: Vec<[&'a str; 4]>, #[serde(borrow)] bids: Vec<[&'a str; 4]>, ts: &'a str, #[serde(rename = "seqId")] seq_id: u64 }
#[derive(Deserialize)]
struct Msg<'a> { event: Option<&'a str>, #[serde(borrow)] arg: Option<Arg<'a>>, #[serde(borrow)] data: Option<Vec<D<'a>>> }

fn op(op: &str, topics: &[String]) -> Vec<String> {
    let args: Vec<_> = topics.iter().map(|t| serde_json::json!({ "channel": "bbo-tbt", "instId": t })).collect();
    vec![serde_json::json!({ "op": op, "args": args }).to_string()]
}

impl Adapter for BboTbt {
    fn url(&self) -> String { URL.into() }
    fn caps(&self) -> Caps { Caps { sequence: false, event_time: true, book: BookKind::TopOfBook, silence: SILENCE } }
    fn subscribe_all(&self, topics: &[String]) -> Vec<String> { op("subscribe", topics) }
    fn unsubscribe(&self, topics: &[String]) -> Vec<String> { op("unsubscribe", topics) }
    // OKX closes a connection that receives nothing for 30 s
    fn heartbeat(&self) -> Option<(Duration, String)> { Some((Duration::from_secs(20), "ping".into())) }

    fn decode(&mut self, text: &str, recv: Stamp, source: SourceId, out: &mut Vec<Event>) -> Result<Option<u64>, DecodeError> {
        if text == "pong" { return Ok(None); }
        let m: Msg = serde_json::from_str(text).map_err(|e| DecodeError(format!("json: {e}")))?;
        match m.event {
            // a refused subscription would leave the connection silent: reported as unreadable instead
            Some("error") => return Err(DecodeError("error event".into())),
            Some(_) => return Ok(None), // subscribe / unsubscribe acks, notices
            None => {}
        }
        let (Some(arg), Some(data)) = (m.arg, m.data) else { return Err(DecodeError("no arg or data".into())) };
        if arg.channel != "bbo-tbt" { return Err(DecodeError(format!("channel {}", arg.channel))); }
        if self.inst.get_or_insert_with(|| arg.inst_id.to_string()) != arg.inst_id { return Err(DecodeError(format!("second instrument {}", arg.inst_id))); }
        for d in data {
            let (Some(b), Some(a)) = (d.bids.first(), d.asks.first()) else { return Err(DecodeError("empty side".into())) };
            let bid = parse_micro(b[0]).map_err(|e| DecodeError(format!("bid: {e:?}")))?;
            let ask = parse_micro(a[0]).map_err(|e| DecodeError(format!("ask: {e:?}")))?;
            let ts = d.ts.parse().map_err(|e| DecodeError(format!("ts: {e}")))?;
            out.push(Event { source, recv, event_ms: Some(ts), seq: Some(d.seq_id), payload: Payload::Quote { bid, ask } });
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::conformance;
    use engine_core::supervisor::{Supervisor, Verdict};
    use proptest::prelude::*;

    fn fixture(name: &str) -> Vec<String> {
        let path = format!("{}/../../fixtures/raw/{name}", env!("CARGO_MANIFEST_DIR"));
        let v: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v.into_iter().map(|m| m.to_string()).collect()
    }
    fn frames() -> Vec<String> { fixture("okx-bbo-tbt-BTC-USDT-SWAP.json") }

    #[test]
    fn real_frames_decode() {
        let ev = conformance::decodes(&mut BboTbt::default(), &frames());
        assert_eq!(ev.len(), frames().len());
        assert_eq!(ev[0].payload, Payload::Quote { bid: 81_049_900_000, ask: 81_050_000_000 });
        assert_eq!((ev[0].event_ms, ev[0].seq), (Some(1_792_359_573_007), Some(346_950_393_832)));
        let spot = conformance::decodes(&mut BboTbt::default(), &fixture("okx-bbo-tbt-BTC-USDT.json"));
        assert_eq!(spot[0].payload, Payload::Quote { bid: 81_089_900_000, ask: 81_090_000_000 });
    }

    #[test]
    fn truncated_frames_are_refused() { for f in frames().into_iter().chain(fixture("okx-events.json")) { conformance::truncations_refused(&mut BboTbt::default(), &f); } }

    #[test]
    fn acks_and_pong_carry_nothing_a_refused_subscription_is_reported() {
        let ev = fixture("okx-events.json");
        assert!(conformance::decodes(&mut BboTbt::default(), &[ev[0].clone(), ev[2].clone(), "pong".into()]).is_empty());
        assert!(BboTbt::default().decode(&ev[1], Stamp { mono_ns: 0, wall_ns: 0 }, 0, &mut vec![]).is_err());
    }

    #[test]
    fn one_instrument_per_source() {
        let mut a = BboTbt::default();
        conformance::decodes(&mut a, &frames());
        assert!(a.decode(&fixture("okx-bbo-tbt-BTC-USDT.json")[0], Stamp { mono_ns: 0, wall_ns: 0 }, 0, &mut vec![]).is_err());
    }

    /// The gap counter is right: the real seqIds jump (book sequence), and no gap is counted nor resync asked for.
    #[test]
    fn jumping_seq_ids_count_no_gap() {
        let mut a = BboTbt::default();
        let mut sup = Supervisor::new(a.caps());
        sup.on_connect(0);
        for (i, f) in frames().iter().enumerate() {
            let seq = a.decode(f, Stamp { mono_ns: i as u64, wall_ns: 0 }, 0, &mut vec![]).unwrap();
            assert_eq!(sup.on_message(i as u64, seq).0, Verdict::Accept);
        }
        assert_eq!((sup.gaps, sup.duplicates), (0, 0));
    }

    #[test]
    fn subscription_frame_matches_the_one_in_use() {
        // the frame of the current application (src/feeds/live.js), known to work
        assert_eq!(BboTbt::default().subscribe_all(&["BTC-USDT-SWAP".into()]), vec![r#"{"args":[{"channel":"bbo-tbt","instId":"BTC-USDT-SWAP"}],"op":"subscribe"}"#]);
    }

    proptest! {
        #[test]
        fn garbage_never_panics(s in ".{0,200}") { conformance::never_panics(&mut BboTbt::default(), &s); }
    }
}
