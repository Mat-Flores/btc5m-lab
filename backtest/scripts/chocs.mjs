// Étude des fenêtres « choc » (docs/recherche/fenetres-choc.md) sur l'export de `backtest run --batch config/chocs.json`.
//   node scripts/chocs.mjs out/chocs.jsonl "C:/dev/REA Projects/btc5m-lag-sim/data/binance"
// Choc : le plus grand |Δ prix| Binance spot sur 10 s dans la fenêtre dépasse le 95e centile des fenêtres d'avril à
// septembre (fichiers BTCUSDT-1s, jamais la semaine testée). Comparaisons en rendement par fenêtre (pnl / capital avant),
// car le capital composé rend les $ de deux runs incomparables. Monte Carlo : bootstrap par blocs d'une heure.
import fs from 'node:fs';
import path from 'node:path';

const [exp, bin] = process.argv.slice(2);
const LAG = 10, Q = 0.95, NBOOT = 10000, CALIB_END = '2026-10-14';
// annonces US à fort impact pendant les packs (8:30 ET = 12:30 UTC, heure d'été US) ; pause de T − 5 min à T + 15 min
const EVENTS = ['2026-10-15T12:30:00Z', '2026-10-20T12:30:00Z', '2026-10-22T12:30:00Z'].map(s => Date.parse(s) / 1000);
const paused = s => EVENTS.some(t => s >= t - 300 && s < t + 900);

const day = d => { const b = fs.readFileSync(path.join(bin, `BTCUSDT-1s-${d}.f64`)); return new Float64Array(b.buffer, b.byteOffset, b.length / 8); };
const maxMove = (p, i0) => { let m = 0; for (let i = i0; i + LAG < i0 + 300; i++) m = Math.max(m, Math.abs(p[i + LAG] - p[i])); return m; };

// seuil sur avril-septembre
const calib = [];
for (const f of fs.readdirSync(bin).filter(f => /^BTCUSDT-1s-\d{4}-\d{2}-\d{2}\.f64$/.test(f) && f.slice(11, 21) < CALIB_END)) {
  const p = day(f.slice(11, 21));
  for (let i = 0; i < 86400; i += 300) calib.push(maxMove(p, i));
}
calib.sort((a, b) => a - b);
const THR = calib[Math.floor(Q * (calib.length - 1))];

const cache = {};
const moveOf = s => { const d = new Date(s * 1000).toISOString().slice(0, 10); cache[d] ??= day(d); return maxMove(cache[d], s % 86400); };

const f64 = h => Buffer.from(h, 'hex').readDoubleBE(0);
const runs = {};
for (const l of fs.readFileSync(exp, 'utf8').split('\n')) {
  if (!l) continue;
  const r = JSON.parse(l);
  (runs[r.id] ??= new Map()).set(r.start, { r: f64(r.pnl) / f64(r.eqBefore), played: r.played });
}
const starts = [...runs.A.keys()].sort((a, b) => a - b);
const shock = new Map(starts.map(s => [s, moveOf(s) > THR]));
const hours = [...new Set(starts.map(s => Math.floor(s / 3600)))];
const byHour = new Map(hours.map(h => [h, starts.filter(s => Math.floor(s / 3600) === h)]));

const pct = x => (100 * x).toFixed(2).padStart(7) + ' %';
const sum = (id, keep, tr = (s, v) => v) => starts.filter(keep).reduce((a, s) => a + tr(s, runs[id].get(s).r), 0);
// bootstrap par blocs d'une heure de Σ (rendement variante − rendement base) sur les fenêtres retenues
function boot(v, b, keep, trV = (s, x) => x) {
  const d = new Map(hours.map(h => [h, byHour.get(h).filter(keep).reduce((a, s) => a + trV(s, runs[v].get(s).r) - runs[b].get(s).r, 0)]));
  let seed = 7; const rnd = () => ((seed = (Math.imul(seed, 1103515245) + 12345) >>> 0) / 2 ** 32);
  const t = [];
  for (let k = 0; k < NBOOT; k++) { let x = 0; for (let i = 0; i < hours.length; i++) x += d.get(hours[Math.floor(rnd() * hours.length)]); t.push(x); }
  t.sort((a, c) => a - c);
  return { lo: t[Math.floor(0.025 * NBOOT)], hi: t[Math.floor(0.975 * NBOOT)], p: t.filter(x => x > 0).length / NBOOT };
}

const nS = starts.filter(s => shock.get(s)).length;
console.log(`fenêtres ${starts.length}, du ${new Date(starts[0] * 1000).toISOString()} au ${new Date(starts.at(-1) * 1000).toISOString()}`);
console.log(`seuil choc : max |Δ ${LAG} s| > ${THR.toFixed(2)} $ (${Q * 100}e centile de ${calib.length} fenêtres avant le ${CALIB_END}) ; chocs ${nS} (${(100 * nS / starts.length).toFixed(1)} %)`);
console.log(`pause macro : ${starts.filter(paused).length} fenêtres (${EVENTS.length} annonces), dont ${starts.filter(s => paused(s) && shock.get(s)).length} chocs\n`);

const isS = s => shock.get(s), notS = s => !shock.get(s), all = () => true, SPLIT = Date.parse('2026-10-20T00:00:00Z') / 1000;
console.log('Σ rendement par fenêtre (en % du capital), et écart à la base avec IC95 bootstrap (blocs 1 h)');
console.log('id       jouées   total     chocs      hors chocs |  Δ total [IC95]                 P(Δ>0) |  Δ chocs [IC95]                 P(Δ>0) | Δ sel     Δ test');
for (const id of Object.keys(runs)) {
  const b = id[0], played = starts.filter(s => runs[id].get(s).played).length;
  let line = `${id.padEnd(7)} ${String(played).padStart(6)} ${pct(sum(id, all))} ${pct(sum(id, isS))} ${pct(sum(id, notS))}`;
  if (id !== b) {
    const ta = boot(id, b, all), ts = boot(id, b, isS);
    const dd = keep => sum(id, keep) - sum(b, keep);
    line += ` | ${pct(dd(all))} [${pct(ta.lo)};${pct(ta.hi)}] ${ta.p.toFixed(2)} | ${pct(dd(isS))} [${pct(ts.lo)};${pct(ts.hi)}] ${ts.p.toFixed(2)} | ${pct(dd(s => s < SPLIT))} ${pct(dd(s => s >= SPLIT))}`;
  }
  console.log(line);
}

console.log('\nPause macro (rendement mis à 0 dans les fenêtres en pause), écart à la base');
for (const b of ['A', 'B']) {
  const z = (s, x) => paused(s) ? 0 : x, t = boot(b, b, all, z);
  console.log(`${b} : rendement des fenêtres en pause ${pct(sum(b, paused))} ; Δ ${pct(-sum(b, paused))} [${pct(t.lo)};${pct(t.hi)}] P(Δ>0) ${t.p.toFixed(2)}`);
}

// walk-forward par jour : chaque jour, la variante (base comprise) au meilleur Σ rendement sur les jours précédents
console.log('\nWalk-forward par jour (choix sur les jours passés, appliqué le jour suivant)');
const days = [...new Set(starts.map(s => new Date(s * 1000).toISOString().slice(0, 10)))];
const dayOf = s => new Date(s * 1000).toISOString().slice(0, 10);
for (const b of ['A', 'B']) {
  const ids = Object.keys(runs).filter(id => id[0] === b);
  let wf = 0, base = 0; const picks = [];
  for (let k = 1; k < days.length; k++) {
    const past = s => days.indexOf(dayOf(s)) < k, today = s => dayOf(s) === days[k];
    const pick = ids.reduce((x, y) => (sum(y, past) > sum(x, past) ? y : x));
    picks.push(`${days[k].slice(5)}:${pick}`);
    wf += sum(pick, today); base += sum(b, today);
  }
  console.log(`${b} : walk-forward ${pct(wf)} contre base ${pct(base)} (jours 2 à ${days.length}) ; choix ${picks.join(' ')}`);
}
