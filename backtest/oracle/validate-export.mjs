// Oracle Node de la non-régression maker : refait `node engine/maker-bt/validate.mjs --delay 0 --sim exact` du dépôt
// source (lecture seule : ses modules sont importés, rien n'y est écrit) et exporte, en plus des mêmes lignes de
// résumé, chaque jumeau rejoué par simWindow avec ses valeurs f64 en bits (hexadécimal), pour une comparaison bit à bit.
//   node --max-old-space-size=8192 backtest/oracle/validate-export.mjs <racine du dépôt source> <sortie.jsonl> [débuts de fenêtres,...] [--maker 0x...]
// --maker : les ordres d'un wallet (arcsine), comme `validate.mjs --maker` (bloc repris de validate.mjs:90-101).
// Les lignes de résumé reprennent celles de validate.mjs:105-112 et 131-133, au caractère près.
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const argv = process.argv.slice(2), mi = argv.indexOf('--maker');
const maker = mi >= 0 ? argv.splice(mi, 2)[1].toLowerCase() : null;
const [src, outFile, only] = argv;
if (!src || !outFile) { console.error('usage: validate-export.mjs <source root> <out.jsonl> [starts]'); process.exit(2); }
const mod = (p) => import(pathToFileURL(path.join(src, 'engine', 'maker-bt', p)).href);
const { twins } = await mod('validate.mjs');
const { simWindow } = await mod('sim.mjs');
const { windows, normalize } = await mod('run.mjs');
const { loadB, readB } = await mod('data.mjs');

const dv = new DataView(new ArrayBuffer(8));
const hex = (x) => { dv.setFloat64(0, x); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const keep = only ? new Set(only.split(',').map(Number)) : null;
const delay = 0;

const arrivals = maker ? (x, recs, firstReal) => recs.findLast((r) => !r[4] && r[1] <= firstReal && r[1] >= firstReal - 300e3 && Math.abs(r[3] - r[2] - x.size) < 0.011) : null;
let byWin = null;
if (maker) {            // the wallet's matches per window, each order tagged with its on-chain hash
  const DATA_DIR = path.join(src, 'data');
  const EX = '0xe111180000d2663c0091e4f400237545b87b996b', read = (n) => fs.readFileSync(path.join(DATA_DIR, 'wallets', `${maker}.${n}.jsonl`), 'utf8').split(String.fromCharCode(10)).filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  const hash = new Map(); for (const r of read('orders')) if (r.taker !== EX) (hash.get(r.tx) ?? hash.set(r.tx, []).get(r.tx)).push(r);
  const win = new Map(); for (const g of fs.readdirSync(path.join(DATA_DIR, 'polymarket', 'windows'))) if (g.startsWith('btc-updown-5m-')) { const w = JSON.parse(fs.readFileSync(path.join(DATA_DIR, 'polymarket', 'windows', g), 'utf8')); for (const t of w.tokens ?? []) win.set(t, w.start); }
  byWin = new Map();
  for (const m of read('calldata')) {
    for (const x of m.makers) x.hash = hash.get(m.tx)?.find((r) => r.token === x.token && Math.abs(r.shares - x.filled) < 0.011)?.order;
    const st = win.get(m.makers[0]?.token); if (st != null) (byWin.get(st) ?? byWin.set(st, []).get(st)).push(m);
  }
}
const all = [];
for (const m of loadB()) { if (keep && !keep.has(m.start)) continue; if (byWin && !byWin.has(m.start)) continue; const t = twins(m.start, { delay, maker, arrivals, matches: byWin?.get(m.start) }); if (t) all.push(...t); }
const ok = all.filter((x) => !x.missing), q = (a, p) => { a = [...a].sort((u, v) => u - v); return a[Math.floor(p * (a.length - 1))]; };
const R = ok.reduce((t, x) => t + x.realSh, 0), P = ok.reduce((t, x) => t + x.predSh, 0);
console.log(`orders ${all.length}, arrival found ${ok.length}; real filled ${R.toFixed(0)} sh, predicted ${P.toFixed(0)} sh (ratio ${(P / R).toFixed(3)})`);
const first = ok.filter((x) => x.pred.length).map((x) => x.pred[0][0] - Math.min(...x.real.map((r) => r[0])));
console.log(`predicted at least one fill: ${(ok.filter((x) => x.pred.length).length / ok.length).toFixed(3)}; first fill predicted - real (ms) q10/50/90 ${[0.1, 0.5, 0.9].map((p) => q(first, p))}`);
const err = ok.map((x) => x.predSh - x.realSh);
console.log(`per order predicted - real (sh) q10/50/90 ${[0.1, 0.5, 0.9].map((p) => q(err, p).toFixed(2))}, exact (±0.02) ${(err.filter((e) => Math.abs(e) < 0.02).length / ok.length).toFixed(3)}`);
const known = ok.filter((x) => x.end !== Infinity || x.realSh >= x.size - 0.011);
const RK = known.reduce((t, x) => t + x.realSh, 0), PK = known.reduce((t, x) => t + x.predSh, 0);
console.log(`orders with a known end (filled or cancel found): ${known.length}; real ${RK.toFixed(0)} predicted ${PK.toFixed(0)} (ratio ${(PK / RK).toFixed(3)})`);

const W = new Map(windows().map((w) => [w.start, w])), base = normalize({});
const tw = ok.filter((x) => x.arrive >= x.start * 1000 && W.has(x.start) && (W.get(x.start).winner === 0 || W.get(x.start).winner === 1) && (x.end !== Infinity || x.realSh >= x.size - 0.011));
const acc = { exact: { pred: 0, exact: 0, pnl: 0 } };
let real = 0, realPnl = 0, cache = new Map();
const day = new Map(), dk = (st) => new Date(st * 1000).toISOString().slice(5, 10);
const out = fs.createWriteStream(outFile);
for (const x of tw) {
  const w = W.get(x.start), d = cache.get(x.start) ?? (cache.clear(), cache.set(x.start, readB(x.start)).get(x.start));
  const pay = (W.get(x.start).winner === x.s ? 1 : 0) - x.q / 1000;
  real += x.realSh; realPnl += x.realSh * pay;
  const D = day.get(dk(x.start)) ?? day.set(dk(x.start), { real: 0 }).get(dk(x.start)); D.real += x.realSh * pay;
  const b = 'exact';
  const strat = { ...base, script: { s: x.s, q: x.q / 1000, r: x.size, j: x.j, end: x.end === Infinity ? Infinity : x.end - x.start * 1000 } };
  const rec = simWindow(w, d, strat, { bound: b, delay }, null);
  const sh = Math.min(x.size, rec.fills[x.s].reduce((t, f) => t + f[1], 0));
  acc[b].pred += sh; acc[b].pnl += sh * pay; D[b] = (D[b] ?? 0) + sh * pay; if (Math.abs(sh - x.realSh) < 0.02) acc[b].exact++;
  out.write(JSON.stringify({ start: x.start, j: x.j, s: x.s, q: x.q, size: hex(x.size), qa0: hex(x.qa0), arrive: hex(x.arrive), end: hex(x.end),
    realSh: hex(x.realSh), sh: hex(sh), fills: rec.fills[x.s].length, winner: w.winner }) + '\n');
}
out.end();
console.log(`simWindow on ${tw.length} twins after the open with a known end (real ${real.toFixed(0)} sh, PnL ${realPnl.toFixed(0)} $):`);
for (const [b, a] of Object.entries(acc)) console.log(`  ${b.padEnd(10)} predicted ${a.pred.toFixed(0)} sh, ratio ${(a.pred / real).toFixed(3)}, orders exact ${(a.exact / tw.length).toFixed(3)}, PnL ${a.pnl.toFixed(0)} $`);
for (const [d, D] of day) console.log(`  ${d} real ${D.real.toFixed(0)} $ | ${Object.keys(acc).map((b) => `${b} ${(D[b] ?? 0).toFixed(0)} $`).join(' | ')}`);
console.log(`bits: pred ${hex(acc.exact.pred)} pnl ${hex(acc.exact.pnl)} real ${hex(real)} realPnl ${hex(realPnl)} exact ${acc.exact.exact}`);
