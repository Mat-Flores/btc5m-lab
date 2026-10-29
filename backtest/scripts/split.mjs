// Mesure E1 : chiffres de `validate` (export par jumeau) séparés en fenêtres d'ajustement et de contrôle (split temporel).
//   node backtest/scripts/split.mjs <export.jsonl> [split ISO, défaut 2026-10-20T00:00:00Z]
// volume = parts prédites / réelles ; exacts = |prédit − réel| < 0,02 part ; PnL = Σ parts × (gain − prix), comme validate.
import { readFileSync } from 'node:fs';

const f64 = (h) => Buffer.from(h, 'hex').readDoubleBE(0);
const [file, splitIso = '2026-10-20T00:00:00Z'] = process.argv.slice(2);
const split = Date.parse(splitIso) / 1000;
const sets = { ajustement: [], controle: [] };
for (const l of readFileSync(file, 'utf8').split('\n')) {
  if (!l) continue;
  const x = JSON.parse(l);
  (x.start < split ? sets.ajustement : sets.controle).push(x);
}
for (const [name, xs] of Object.entries(sets)) {
  let real = 0, pred = 0, exact = 0, pnlR = 0, pnlP = 0;
  for (const x of xs) {
    const r = f64(x.realSh), p = f64(x.sh), pay = (x.winner === x.s ? 1 : 0) - x.q / 1000;
    real += r; pred += p; pnlR += r * pay; pnlP += p * pay;
    if (Math.abs(p - r) < 0.02) exact++;
  }
  console.log(`${name.padEnd(10)} ${xs.length} jumeaux : volume ${(pred / real).toFixed(3)}, exacts ${(exact / xs.length).toFixed(3)}, PnL prédit ${pnlP.toFixed(0)} $ / réel ${pnlR.toFixed(0)} $ (${(100 * (pnlP / pnlR - 1)).toFixed(1)} %)`);
}
