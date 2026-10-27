// Signing vectors from the official SDK (@polymarket/client 0.13.0, installed at the repo root): FAK market orders of a
// Deposit Wallet (signature type 3), with a throwaway key. The network is fully simulated: GETs answer the real
// responses captured in clob-market.json, POSTs are recorded and refused, so nothing leaves the machine. The engine's
// tests rebuild every order, signature, body and HMAC header from these and must find the same bytes.
// Usage: node engine/fixtures/sign/vectors.mjs > engine/fixtures/sign/vectors.json
import { createSecureClient, OrderSide, OrderType } from '@polymarket/client';
import { privateKey } from '@polymarket/client/viem';
import fs from 'node:fs';

const KEY = '0x' + '11'.repeat(32);   // throwaway, public on purpose
const CREDS = { apiKey: '00000000-0000-4000-8000-000000000000', secret: 'c2VjcmV0c2VjcmV0c2VjcmV0c2VjcmV0c2VjcmV0MDA=', passphrase: 'pass' };
const M = JSON.parse(fs.readFileSync(new URL('./clob-market.json', import.meta.url)));
let mts, posted = [], typed = [], auth = [];
globalThis.fetch = async (u, o = {}) => {
  const r = u instanceof Request ? u : new Request(u, o), url = new URL(r.url);
  if (r.method === 'GET' && url.pathname.startsWith('/markets-by-token/')) return Response.json(M.markets_by_token);
  if (r.method === 'GET' && url.pathname.startsWith('/clob-markets/')) return Response.json({ ...M.clob_markets, mts });
  if (r.method === 'GET' && url.pathname === '/deployed') return Response.json({ deployed: true });
  if (r.method === 'POST' && url.pathname.startsWith('/auth/')) { auth.push({ path: url.pathname, headers: Object.fromEntries(r.headers) }); return Response.json(CREDS); }
  if (r.method === 'POST' && url.pathname === '/order') { posted.push({ body: await r.text(), headers: Object.fromEntries(r.headers) }); return Response.json({ error: 'simulated' }, { status: 400 }); }
  throw new Error(`unexpected request ${r.method} ${r.url}`);
};
const base = privateKey(KEY);
const signer = { getAddress: () => base.getAddress(), signMessage: (m) => base.signMessage(m),
  async signTypedData(p) { const s = await base.signTypedData(p); typed.push(JSON.parse(JSON.stringify(p, (_, v) => typeof v === 'bigint' ? v.toString() : v))); return s; } };

// deterministic inputs (xorshift), so a regenerated file differs only by salts and timestamps
let x = 0x2545f491;
const rnd = () => { x ^= x << 13; x ^= x >>> 17; x ^= x << 5; return (x >>> 0) / 2 ** 32; };
const token = M.markets_by_token.secondary_token_id;
const out = { key: KEY, creds: CREDS, signer: await signer.getAddress(), cases: [] };
for (const tick of [0.01, 0.001]) {
  mts = tick;
  // the wallet is left out: the SDK derives the signer's Deposit Wallet, as for the real account
  typed = [];
  const c = await createSecureClient({ signer });
  if (!out.auth) out.auth = { typed: typed[0], request: auth[0] };
  out.wallet = c.account?.wallet ?? out.wallet;
  const dec = tick === 0.01 ? 2 : 3;
  for (let i = 0; i < 40; i++) {
    const buy = i % 2 === 0;
    const price = +(Math.max(tick, Math.min(1 - tick, Math.round((0.02 + rnd() * 0.96) / tick) * tick))).toFixed(dec);
    const size = (buy ? 1 + rnd() * 49 : 5 + rnd() * 95).toFixed(2);
    const req = buy ? { assetId: token, side: OrderSide.BUY, amount: size, maxPrice: price, orderType: OrderType.FAK }
      : { assetId: token, side: OrderSide.SELL, shares: size, minPrice: price, orderType: OrderType.FAK };
    posted = []; typed = [];
    const order = await c.createMarketOrder(req);
    try { await c.postOrder(order); } catch { /* refused by the simulated network: only the request is wanted */ }
    out.cases.push({ tick, side: buy ? 'BUY' : 'SELL', size, price, typed: typed[0], signature: order.signature, request: posted[0] });
  }
}
process.stdout.write(JSON.stringify(out, null, 1) + '\n');
