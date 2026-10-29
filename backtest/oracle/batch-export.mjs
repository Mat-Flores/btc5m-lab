// Oracle Node de la non-régression maker sur un lot de l'ancien format (`engine/maker-bt/batches/*.json`) : chaque
// candidat construit comme `explore.mjs` (build/setPath, lecture seule : rien n'est écrit dans le dépôt source, pas
// d'EXPLORATION.md), rejoué par `runMany` du dépôt source sur les fenêtres avant CONFIRM, variante P seulement
// (borne et retenue données). Exporte une ligne par (candidat, fenêtre), valeurs f64 en bits (hexadécimal).
//   node --max-old-space-size=8192 backtest/oracle/batch-export.mjs <racine du dépôt source> <lot.json> <borne> <retenue s> <sortie.jsonl> [débuts,...]
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const [src, batchFile, bound, delayArg, outFile, only] = process.argv.slice(2);
if (!outFile) { console.error('usage: batch-export.mjs <source root> <batch.json> <bound> <delay s> <out.jsonl> [starts]'); process.exit(2); }
const { CONFIRM, normalize, runMany, windows } = await import(pathToFileURL(path.join(src, 'engine', 'maker-bt', 'run.mjs')).href);

const dv = new DataView(new ArrayBuffer(8));
const hex = (x) => { dv.setFloat64(0, x); return dv.getBigUint64(0).toString(16).padStart(16, '0'); };
const batch = JSON.parse(fs.readFileSync(batchFile, 'utf8'));
// explore.mjs:25-26, à l'identique
const setPath = (o, k, v) => { const ks = k.split('.'); let x = o; for (const p of ks.slice(0, -1)) x = x[p] = { ...(x[p] ?? {}) }; x[ks.at(-1)] = v; return o; };
const build = (c) => { let s = structuredClone(batch.base ?? {}); for (const [k, v] of Object.entries(c)) if (k !== 'id' && k !== 'desc') s = k.includes('.') ? setPath(s, k, v) : Object.assign(s, { [k]: v }); return normalize(s); };
const keep = only ? new Set(only.split(',').map(Number)) : null;
const wins = windows().filter((w) => w.start < CONFIRM && (!keep || keep.has(w.start)));
const cands = batch.candidates;
const t0 = Date.now();
const res = runMany(wins, cands.map((c) => ({ strat: build(c), opts: { bound, delay: Number(delayArg) } })));
const out = fs.createWriteStream(outFile);
res.forEach((recs, i) => {
  for (const r of recs) {
    const f = (x) => (x === undefined ? null : hex(x));
    out.write(JSON.stringify({ id: cands[i].id, start: r.start, why: r.why, played: r.played, eqBefore: f(r.eqBefore), eqAfter: f(r.eqAfter), pnl: f(r.pnl),
      spent: f(r.spent), payout: f(r.payout), proceeds: f(r.proceeds), fees: f(r.fees), sh: r.sh ? r.sh.map(hex) : null, nf: r.nf ?? null,
      driftS: f(r.driftS), driftN: f(r.driftN) }) + '\n');
  }
  const last = recs.at(-1);
  console.log(`${cands[i].id}: ${recs.length} windows, played ${recs.filter((r) => r.played).length}, final ${last.eqAfter.toFixed(4)} (${hex(last.eqAfter)})`);
});
out.end();
console.log(`${wins.length} windows, ${cands.length} candidates, bound ${bound}, delay ${delayArg}: ${((Date.now() - t0) / 1000).toFixed(0)} s`);
