// Validation des trades `last_trade_price` des traces courantes (borne `exact` du mode trace) contre les fills publics
// complets de data-api (méthode de fetch-fills.mjs : lignes maker, au prix exact du niveau où elles reposaient).
//   node backtest/scripts/ltp-check.mjs <racine trace> <AAAA-MM-JJ>
// Tout est ramené au carnet Up : un ordre sur Down au prix p est le niveau Up opposé à 1 − p. Pour chaque fenêtre
// entièrement couverte par la trace :
// - couverture : part des transactions preneuses de data-api retrouvées (même hash) parmi les last_trade_price ;
// - volume par niveau : vérité = lignes maker de data-api ; mesuré = taille des last_trade_price, au niveau que le
//   preneur consomme (achat : ask, vente : bid). Accord = Σ min(vérité, mesuré) sur les niveaux.
// Aucune dépendance ; data-api interrogée à 5 requêtes/s au plus ; rien n'est écrit.
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import readline from 'node:readline';

const [root, day] = process.argv.slice(2);
if (!root || !day) { console.error('usage: ltp-check.mjs <trace root> <YYYY-MM-DD>'); process.exit(2); }
const file = (src) => path.join(root, src, day.slice(0, 4), day.slice(5, 7), `${day}.jsonl.gz`);
const lines = (f) => readline.createInterface({ input: fs.createReadStream(f).pipe(zlib.createGunzip()), crlfDelay: Infinity });

// fenêtres : conditionId → { start, up, down } (gamma)
const win = new Map();
for await (const l of lines(file('polymarket-gamma'))) {
  const x = JSON.parse(l); if (x.kind !== 'http' || !x.raw) continue;
  let evs; try { evs = JSON.parse(x.raw); } catch { continue; }
  const m = evs?.[0]?.markets?.[0]; if (!m) continue;
  const tokens = JSON.parse(m.clobTokenIds), outs = JSON.parse(m.outcomes), u = outs.indexOf('Up');
  win.set(m.conditionId, { start: Number(evs[0].slug.split('-').pop()), up: tokens[u], down: tokens[1 - u] });
}
// last_trade_price par marché, et bornes de la trace
const ltp = new Map();
let t0 = Infinity, t1 = -Infinity;
for await (const l of lines(file('polymarket-clob'))) {
  const x = JSON.parse(l);
  if (typeof x.t_recv === 'number' && x.raw) { t0 = Math.min(t0, x.t_recv); t1 = Math.max(t1, x.t_recv); }
  if (x.kind !== 'trade') continue;
  const e = JSON.parse(x.raw); if (e.event_type !== 'last_trade_price') continue;
  (ltp.get(e.market) ?? ltp.set(e.market, []).get(e.market)).push(e);
}
// niveau du carnet Up consommé par un ordre (asset, côté, prix) : [côté Up 'B'|'A', prix en millièmes]
const level = (w, asset, side, price) => {
  const p = Math.round(Number(price) * 1000), up = asset === w.up;
  const book = side === 'BUY' ? 'A' : 'B';                         // un achat consomme les asks, une vente les bids
  return up ? `${book}${p}` : `${book === 'A' ? 'B' : 'A'}${1000 - p}`;
};

let next = 0;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const get = async (u) => {
  for (let a = 0; ; a++) {
    const now = Date.now(), at = Math.max(now, next); next = at + 200; if (at > now) await sleep(at - now);
    try { const r = await fetch(u, { signal: AbortSignal.timeout(30000) }); if (r.ok) return r.json(); } catch {}
    if (a > 6) throw new Error(u);
    await sleep(1000 * 2 ** a);
  }
};

const tot = { wins: 0, trunc: 0, apiTx: 0, found: 0, ltpTx: 0, truth: 0, meas: 0, agree: 0, sizeEq: 0 };
for (const [cid, w] of win) {
  if (w.start * 1e6 < t0 || (w.start + 300) * 1e6 > t1 - 600e6) continue;   // fenêtre (et 10 min après) dans la trace
  const tk = new Map(), rows = [];
  for (let off = 0; ; off += 500) {
    const p = await get(`https://data-api.polymarket.com/trades?market=${cid}&limit=500&offset=${off}&takerOnly=true`);
    for (const r of p) tk.set(`${r.transactionHash}|${r.proxyWallet}|${r.asset}|${r.size}`, r);
    if (p.length < 500 || off >= 9500) break;
  }
  let truncated = false;
  for (let off = 0; ; off += 500) {
    if (off > 9500) { truncated = true; break; }
    const p = await get(`https://data-api.polymarket.com/trades?market=${cid}&limit=500&offset=${off}&takerOnly=false`);
    rows.push(...p);
    if (p.length < 500) break;
  }
  if (truncated) { tot.trunc++; continue; }
  tot.wins++;
  const truth = new Map(), meas = new Map(), add = (m, k, v) => m.set(k, (m.get(k) ?? 0) + v);
  for (const r of rows) if (!tk.has(`${r.transactionHash}|${r.proxyWallet}|${r.asset}|${r.size}`)) add(truth, level(w, r.asset, r.side === 'BUY' ? 'SELL' : 'BUY', r.price), r.size);
  const L = ltp.get(cid) ?? [], byTx = new Map(L.map((e) => [e.transaction_hash, e]));
  for (const e of L) add(meas, level(w, e.asset_id, e.side, e.price), Number(e.size));
  const takers = [...tk.values()];
  tot.apiTx += takers.length; tot.ltpTx += L.length;
  for (const r of takers) { const e = byTx.get(r.transactionHash); if (e) { tot.found++; if (Math.abs(Number(e.size) - r.size) < 0.011) tot.sizeEq++; } }
  for (const v of truth.values()) tot.truth += v;
  for (const v of meas.values()) tot.meas += v;
  for (const [k, v] of truth) tot.agree += Math.min(v, meas.get(k) ?? 0);
}
const pc = (a, b) => (100 * a / b).toFixed(1) + ' %';
console.log(`${day} : ${tot.wins} fenêtres couvertes (${tot.trunc} tronquées par data-api, écartées)`);
console.log(`transactions preneuses data-api ${tot.apiTx} ; retrouvées dans last_trade_price ${tot.found} (${pc(tot.found, tot.apiTx)}), dont même taille ${pc(tot.sizeEq, tot.found)} ; last_trade_price ${tot.ltpTx}`);
console.log(`volume par niveau du carnet Up : vérité ${tot.truth.toFixed(0)} parts, last_trade_price ${tot.meas.toFixed(0)} parts, accord ${tot.agree.toFixed(0)} (rappel ${pc(tot.agree, tot.truth)}, précision ${pc(tot.agree, tot.meas)})`);
