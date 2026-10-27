// Phase 7 fixture: a real book drop taken from a recorded session, for the stop loss replay test
// (core/tests/stop_loss_replay.rs). Rebuilds every Polymarket book of the session from its `book` and `delta` records,
// opens a test position (10 shares at the best ask) every 10 s on each token, and keeps the first drop that takes the
// latent result to -50 % of the cost before the last minute of the window. The expected instant and values are
// computed here, independently of the Rust code, with exact integers (BigInt), from the formulas of 02-conception.md §6:
//   amount = size x price ; fee = size x rate x p x (1 - p)^e ; both rounded half up to the micro-unit
//   exit = bids walked from the best, level by level, each level's amount minus its fee
// Usage: node engine/bench/book-drop.mjs <session.jsonl.gz> [out.json]
import fs from 'node:fs';
import zlib from 'node:zlib';
import readline from 'node:readline';
import path from 'node:path';

const [file, out = path.join(import.meta.dirname, '..', 'fixtures', 'sessions', 'book-drop.json')] = process.argv.slice(2);
const M = 1_000_000n, LOSS = 500_000n, QUIET_MS = 60_000, SHARES = 10n * M, EVERY_MS = 10_000, MIN_DROP_MS = 5_000;

// exact decimal string -> micro-units; the recorder wrote numbers with JSON.stringify, whose text is the shortest exact form
const micro = (x) => { const [i, f = ''] = String(x).split('.'); if (f.length > 6) throw new Error(`too precise: ${x}`); return BigInt(i) * M + BigInt(f.padEnd(6, '0')); };
const amount = (size, price) => (size * price + M / 2n) / M;
const fee = (size, p, rate, e) => { const den = M ** BigInt(2 + e); return (size * rate * p * (M - p) ** BigInt(e) + den / 2n) / den; };
function exit(shares, bids, rate, e) {
  let left = shares, net = 0n;
  for (const [p, s] of [...bids].sort((a, b) => (a[0] < b[0] ? 1 : -1))) {
    if (left === 0n) break;
    const take = s < left ? s : left;
    net += amount(take, p) - fee(take, p, rate, e);
    left -= take;
  }
  return { net, unfilled: left };
}
const bestAsk = (asks) => [...asks.keys()].reduce((m, p) => (m === null || p < m ? p : m), null);

async function* records() {
  const rl = readline.createInterface({ input: fs.createReadStream(file).pipe(zlib.createGunzip()).on('error', () => rl.close()) });
  try { for await (const line of rl) { try { yield JSON.parse(line); } catch { /* torn last line */ } } } catch { /* truncated gzip */ }
}

// pass 1: find the drop
const windows = new Map(); // token -> { end, rate, e }
const books = new Map(); // token -> { bids: Map, asks: Map }
const cands = new Map(); // token -> [{ t0, price, cost, lastOpen }]
let found = null;
const lastOpen = new Map();
function touched(token, t) {
  const w = windows.get(token), b = books.get(token);
  if (!w || !b) return;
  const list = cands.get(token) ?? [];
  for (const c of list) {
    if (c.fired || t + QUIET_MS >= w.end) continue;
    const { net } = exit(SHARES, b.bids, w.rate, w.e);
    if ((net - c.cost) * M > -LOSS * c.cost) continue;
    c.fired = true; // only its first trigger counts, and it must come from a drop, not from the entry spread
    if (t - c.t0 >= MIN_DROP_MS && (!found || t - c.t0 < found.t - found.t0)) found = { token, t0: c.t0, t, price: c.price };
  }
  const ask = bestAsk(b.asks);
  if (ask !== null && ask >= 300_000n && ask <= 800_000n && t + 120_000 < w.end && t - (lastOpen.get(token) ?? -Infinity) >= EVERY_MS) {
    lastOpen.set(token, t);
    list.push({ t0: t, price: ask, cost: amount(SHARES, ask) + fee(SHARES, ask, w.rate, w.e) });
    cands.set(token, list);
  }
}
function apply(r, onTouch) {
  if (r.k === 'meta' && r.w?.tokens) for (const tk of r.w.tokens) windows.set(tk, { end: (r.w.start + r.w.len) * 1000, rate: micro(r.w.fee.rate), e: r.w.fee.exponent });
  if (r.k === 'book') { books.set(r.a, { bids: new Map(r.bids.map(([p, s]) => [micro(p), micro(s)])), asks: new Map(r.asks.map(([p, s]) => [micro(p), micro(s)])) }); onTouch(r.a, r.t, r); }
  if (r.k === 'delta') {
    const seen = new Set();
    for (const [a, side, p, s] of r.ch) {
      const b = books.get(a); if (!b) continue;
      const m = side === 'B' ? b.bids : b.asks;
      if (micro(s) === 0n) m.delete(micro(p)); else m.set(micro(p), micro(s));
      seen.add(a);
    }
    for (const a of seen) onTouch(a, r.t, r);
  }
}
for await (const r of records()) apply(r, (a, t) => touched(a, t));
if (!found) { console.error('no drop found'); process.exit(1); }

// pass 2: the token's book from the entry to a little after the trigger, in the test's format
windows.clear(); books.clear();
const w = () => windows.get(found.token);
const events = [];
let expected = null, after = 0;
const str = (x) => { const s = (x / M).toString() + '.' + (x % M).toString().padStart(6, '0'); return s.replace(/\.?0+$/, ''); };
for await (const r of records()) {
  if (after > 30) break;
  apply(r, (a, t, rec) => {
    if (a !== found.token || t < found.t0) return;
    const b = books.get(a);
    if (!events.length) events.push({ t, bids: [...b.bids].map(([p, s]) => [str(p), str(s)]), asks: [...b.asks].map(([p, s]) => [str(p), str(s)]) });
    else if (rec.k === 'book') events.push({ t, bids: rec.bids.map(([p, s]) => [String(p), String(s)]), asks: rec.asks.map(([p, s]) => [String(p), String(s)]) });
    else events.push({ t, changes: rec.ch.filter((c) => c[0] === a).map(([, side, p, s]) => [side, String(p), String(s)]) });
    if (expected) { after++; return; }
    const cost = amount(SHARES, found.price) + fee(SHARES, found.price, w().rate, w().e);
    const { net, unfilled } = exit(SHARES, b.bids, w().rate, w().e);
    if (t + QUIET_MS < w().end && (net - cost) * M <= -LOSS * cost) expected = { index: events.length - 1, t, cost: Number(cost), net: Number(net), latent: Number(net - cost), unfilled: Number(unfilled) };
  });
}
const fx = {
  source: path.basename(file), token: found.token, end_ms: w().end, fee_rate: str(w().rate), fee_exponent: w().e,
  entry: { t: found.t0, size: str(SHARES), price: str(found.price), note: 'test position, not a real trade' },
  stop_loss: { loss: Number(LOSS), quiet_s: QUIET_MS / 1000 }, expected, events,
};
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify(fx));
console.log(JSON.stringify({ out, events: events.length, entry: fx.entry, expected, seconds_to_trigger: (expected.t - found.t0) / 1000, ms_before_end: w().end - expected.t }));
