// Auto-redemption beside the live engine (phase 10): every 30 s, the markets the engine traded, resolved and won
// (facts `trade` and `resolved` of its ledger) are redeemed through Polymarket's gasless relayer, with the official
// SDK (`@polymarket/client` 0.13, `redeemPositions`). Only those markets: redeeming any
// other would raise the balance with no fact behind it, and the engine's reconciliation would rightly stop opening.
// The engine sees each redemption at data-api (`REDEEM`) and books it; this process writes nothing but its own log.
// Usage: node redeem.mjs <engine ledger> <wallet file> <log> <seconds>   (passphrase: first line of stdin)
import crypto from 'node:crypto';
import fs from 'node:fs';
import readline from 'node:readline';
import { createSecureClient } from '@polymarket/client';
import { privateKey } from '@polymarket/client/viem';

const [ledger, walletFile, logFile, seconds] = process.argv.slice(2);
const log = (o) => fs.appendFileSync(logFile, JSON.stringify({ t: new Date().toISOString(), ...o }) + '\n');

// the current application's format: scrypt N=2^17 r=8 p=1, AES-256-GCM (src/exec/wallets.js)
function decrypt(payload, pass) {
  const [v, salt, iv, tag, ct] = payload.split('.');
  if (v !== 'v1' || !ct) throw new Error('format de clé chiffrée inconnu');
  const key = crypto.scryptSync(pass, Buffer.from(salt, 'base64url'), 32, { N: 2 ** 17, r: 8, p: 1, maxmem: 256 * 1024 * 1024 });
  const d = crypto.createDecipheriv('aes-256-gcm', key, Buffer.from(iv, 'base64url'));
  d.setAuthTag(Buffer.from(tag, 'base64url'));
  try { return Buffer.concat([d.update(Buffer.from(ct, 'base64url')), d.final()]).toString('utf8').trim(); } catch { throw new Error('phrase secrète incorrecte'); }
}

/** Markets to redeem: traded by the engine, resolved for a token it bought, not seen redeemed. */
export function due(lines) {
  const bought = new Set(), won = new Set(), redeemed = new Set();
  for (const l of lines) {
    let f; try { f = JSON.parse(l).fact; } catch { continue; }
    if (f?.type === 'trade' && f.dir === 'buy' && f.status !== 'Failed') bought.add(`${f.market} ${f.token}`);
    if (f?.type === 'resolved') won.add(`${f.market} ${f.winner}`);
    if (f?.type === 'redeemed') redeemed.add(f.market);
  }
  return [...won].filter((k) => bought.has(k)).map((k) => k.split(' ')[0]).filter((m) => !redeemed.has(m));
}

if (process.argv[1]?.endsWith('redeem.mjs') && ledger) {
  const pass = await new Promise((ok) => readline.createInterface({ input: process.stdin }).once('line', ok));
  const [address, payload] = fs.readFileSync(walletFile, 'utf8').trim().split('=').pop().split(':');
  const client = await createSecureClient({ signer: privateKey(decrypt(payload, pass)), wallet: address });
  log({ type: 'armed', wallet: address });
  const tried = new Map(); // market -> attempts; a market that keeps failing is left to the site after 5
  const end = Date.now() + Number(seconds) * 1000;
  while (Date.now() < end) {
    const lines = fs.existsSync(ledger) ? fs.readFileSync(ledger, 'utf8').split('\n') : [];
    for (const conditionId of due(lines)) {
      const n = tried.get(conditionId) ?? 0;
      if (n >= 5) continue;
      tried.set(conditionId, n + 1);
      try {
        const h = await client.redeemPositions({ conditionId });
        const out = await h.wait();
        log({ type: 'redeemed', market: conditionId, tx: out?.transactionHash ?? null });
        tried.set(conditionId, 5); // done: the engine books it when data-api lists it
      } catch (e) {
        log({ type: 'error', market: conditionId, attempt: n + 1, error: String(e?.message ?? e).slice(0, 500) });
      }
    }
    await new Promise((r) => setTimeout(r, 30_000));
  }
  log({ type: 'end' });
  process.exit(0);
}
