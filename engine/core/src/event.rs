//! Normalized events: what every feed adapter turns its native messages into. Nothing here names a source.
use crate::clock::Stamp;
use crate::money::Micro;
use serde::Serialize;
use std::time::Duration;

/// Index of a source in the engine's configuration; its name lives in the configuration only.
pub type SourceId = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Side { Bid, Ask }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Level { pub price: Micro, pub size: Micro }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Change {
    pub asset: String,
    pub side: Side,
    pub price: Micro,
    /// New total size at this price; 0 removes the level.
    pub size: Micro,
    /// Best prices the source says the book has after this change, when it says so (integrity check).
    pub best_bid: Option<Micro>,
    pub best_ask: Option<Micro>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Payload {
    /// Top of book of a reference market (an exchange price).
    Quote { bid: Micro, ask: Micro },
    /// Full book of one asset: replaces whatever was known.
    /// `tick` absent: the source did not repeat it; the known grid is kept.
    BookSnapshot { asset: String, bids: Vec<Level>, asks: Vec<Level>, tick: Option<Micro> },
    BookDelta { changes: Vec<Change> },
    /// The price grid changed: the book must be rebuilt from a new snapshot.
    TickChange { asset: String, tick: Micro },
    Trade { asset: String, price: Micro, size: Micro, side: Side, id: String },
    /// A reference price published by an oracle.
    Oracle { value: Micro },
    /// A time-weighted average published by an oracle.
    Twap { value: Micro, window_s: u32 },
    /// A trade of one of our own orders, with the venue's size and price.
    Fill { order: String, trade: String, status: crate::order::TradeStatus, size: Micro, price: Micro },
    /// The venue closed one of our orders: nothing more will match.
    OrderClosed { order: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Event {
    pub source: SourceId,
    pub recv: Stamp,
    /// The source's own timestamp, Unix milliseconds, when it gives one.
    pub event_ms: Option<u64>,
    /// The source's sequence number, when it numbers its messages.
    pub seq: Option<u64>,
    pub payload: Payload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookKind { None, TopOfBook, Full }

/// What a source declares about itself; the core does the rest with it.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub sequence: bool,
    pub event_time: bool,
    pub book: BookKind,
    /// No message for this long means the source is invalid (calibrated on recorded data).
    pub silence: Duration,
}
