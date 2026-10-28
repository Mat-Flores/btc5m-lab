// Reference cases of the maker strategy port (strategy/src/maker.rs), computed by the original simulator.
// Usage: node engine/fixtures/maker/gen.mjs <absolute path of btc5m-lag-sim/engine/maker-bt/sim.mjs> > engine/fixtures/maker/cases.json
// One synthetic window (deterministic xorshift): OKX mids from -360 s, Chainlink prints, an Up book from -60 s with
// sweeps, a fine-tick level and a silence. Each case runs `simWindow` (bound prudent, delay 0.15 s); every float is
// written as the hex of its 64 bits, so the Rust test compares bit for bit.
import { pathToFileURL } from 'node:url';

const { simWindow } = await import(pathToFileURL(process.argv[2]).href);
const hex = (x) => { const b = new DataView(new ArrayBuffer(8)); b.setFloat64(0, x); return b.getBigUint64(0).toString(16).padStart(16, '0'); };

let x = 0x2545f491;
const rnd = () => { x ^= x << 13; x ^= x >>> 17; x ^= x << 5; return (x >>> 0) / 2 ** 32; };

// OKX mid every ~0.7 s, cents of a random walk
const px = [];
let cents = 8_100_000;
for (let t = -360_000; t <= 310_000; t += 500 + Math.floor(rnd() * 400)) {
  cents += Math.floor(rnd() * 21) - 10;
  px.push(t + 0.25, cents / 100);
}
const pxAt = (t) => { let p = NaN; for (let i = 0; i < px.length; i += 2) { if (px[i] <= t) p = px[i + 1]; else break; } return p; };
// Chainlink every ~1 s, observed 0.8-1.1 s before, about 25 $ under OKX
const cl = [];
for (let t = -359_000; t <= 360_000; t += 900 + Math.floor(rnd() * 200)) {
  const ts = t - 800 - Math.floor(rnd() * 300);
  cl.push(t + 0.5, ts, Math.round((pxAt(ts) - 25 + (rnd() - 0.5) * 4) * 100) / 100);
}
// Up book: 6 levels each side around a mid that walks in cents; occasional sweeps through the mid
const book = [];
let mid = 50, snapAt = -60_000;
const levels = (t) => {
  book.push(t, t - 20, 0, 0, 0);
  for (let k = 1; k <= 6; k++) book.push(t, t - 20, 1, (mid - k) / 100, 10 + k * 7);
  for (let k = 1; k <= 6; k++) book.push(t, t - 20, 2, (mid + k) / 100, 12 + k * 5);
};
levels(snapAt);
for (let t = -59_800; t <= 305_000; t += 150 + Math.floor(rnd() * 200)) {
  if (t > 200_000 && t < 204_500) continue;                              // a silence of the book (> 3 s)
  const ts = t - 15 - Math.floor(rnd() * 20);
  const r = rnd();
  if (r < 0.04) { mid = Math.min(85, Math.max(15, mid + (rnd() < 0.5 ? -1 : 1))); levels(t); continue; }   // new snapshot
  if (r < 0.10) { const k = 1 + Math.floor(rnd() * 8); book.push(t, ts, 2, (mid - k) / 100, 3 + Math.floor(rnd() * 20)); continue; }   // a seller sweeps down
  if (r < 0.16) { const k = 1 + Math.floor(rnd() * 8); book.push(t, ts, 1, (mid + k) / 100, 3 + Math.floor(rnd() * 20)); continue; }   // a buyer sweeps up
  if (r < 0.17 && t > 150_000) { book.push(t, ts, 2, (mid + 0.5) / 100, 4); continue; }   // a fine-tick level
  const side = rnd() < 0.5 ? 1 : 2, k = 1 + Math.floor(rnd() * 6);
  book.push(t, ts, side, (side === 1 ? mid - k : mid + k) / 100, rnd() < 0.15 ? 0 : 5 + Math.floor(rnd() * 60));
}

const W = { start: 1_792_359_300, winner: 0, fee: { rate: 0.07, exponent: 1, rebateRate: 0.2 }, tick0: 0.01, trades: new Float32Array(0) };
const cases = [
  ['default', {}, 50],
  ['x01-like', { price: { mode: 'offset', offset: 0.01 }, reserveInFlight: true, liqCap: false, quoteEnd: 0, sideProb: [0.3, 0.999], maxDiverge: 0.08 }, 40],
  ['shares-penny-freeze', { shares: 20, orderShares: 7, requote: 'freeze', price: { mode: 'penny', offset: 0.02 }, life: 4000 }, null],
  ['book-join-knife', { price: { mode: 'book', offset: 0.01 }, join: 1, noKnife: { T: 5, M: 0.02 }, pairEdge: 0.02, maxImbalance: 15, pullSide: { J: 5, S: 3 } }, 60],
  ['pull-loser-mid', { pull: { J: 0.6, S: 2 }, cancelLoser: { X: 120, Y: 0.52 }, prob: 'mid', entryFrom: 0 }, 50],
  ['vol-stop', { volMax: 0.5 }, 50],
  ['fixed', { price: { mode: 'fixed', fixed: 0.45 }, shares: 10 }, null],
];
const DEFAULT_PRICE = { mode: 'offset', offset: 0.03, fixed: 0.45 };
const out = { px: px.map(hex), cl: cl.map(hex), book: book.map(hex), start: W.start, cases: [] };
for (const [name, s, budget] of cases) {
  const { DEFAULTS } = await import(pathToFileURL(process.argv[2]).href);
  const strat = { ...DEFAULTS, ...s, price: { ...DEFAULT_PRICE, ...(s.price ?? {}) } };
  const d = { book: Float64Array.from(book), px: Float64Array.from(px), cl: Float64Array.from(cl) };
  const r = simWindow({ ...W }, d, strat, { bound: 'prudent', delay: 0.15 }, budget);
  out.cases.push({ name, strat: s, budget, expect: {
    why: r.why, posted: r.posted, silentSkips: r.silentSkips, pulled: r.pulled, capped: r.capped, loserFilledBefore: r.loserFilledBefore,
    cancelLoserAt: r.cancelLoserAt == null ? null : hex(r.cancelLoserAt), sigma0: hex(r.sigma0),
    spent: hex(r.spent), rebates: hex(r.rebates), pnl: hex(r.pnl), drift: r.drift.map(hex), posN: r.posN.map(hex),
    fills: r.fills.map((f) => f.map((row) => row.map(hex))), joins: r.joins.map((j) => j.map(hex)), fairSec: [...r.fairSec].map(hex),
  } });
}
console.log(JSON.stringify(out));
