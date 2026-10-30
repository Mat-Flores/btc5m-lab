// node --test copy-trading/ui/model.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { createModel, ingest, tiles, markets, cascade, windowStart, performance, myPosition } from './model.js';

const J = [
  { t: 'start', at: 0, mode: 'paper', cash: 20 },
  { t: 'trade', at: 5, tx: 'old', token: '1', side: 'BUY' }, // run précédent : oublié
  { t: 'start', at: 1000, mode: 'paper', wallet: '0xabc', cash: 100, strategy: { cancel_after_min: 60 } },
  { t: 'state', at: 1000, his_value: 1000, real: { value: 100, pnl: 0 }, obs: { value: 100, pnl: 0 } },
  { t: 'order', ev: 'post', at: 2100, id: 1, token: '7', side: 'BUY', price: 0.07, size: 40, ahead: 100, filled: 0 },
  { t: 'trade', at: 2100, tx: '0xa', token: '7', side: 'BUY', his_price: 0.07, his_usd: 3, his_shares: 42.9, src: 'chain',
    lat: { src: 900, obs: 2000, dec: 2050 }, decision: 'buy', order: { id: 1 }, his_order: { price: 0.07, fills: 1, usd: 3 } },
  { t: 'state', at: 61000, his_value: 1010, real: { value: 99, pnl: -1 }, obs: { value: 101, pnl: 1 }, orders: [{ id: 1, ahead: 30 }], real_pos: {}, marks: { 7: 0.075 } },
  { t: 'order', ev: 'fill', at: 70000, id: 1, shares: 40, price: 0.07, left: 0 },
  { t: 'order', ev: 'done', at: 70000, id: 1 },
  { t: 'trade', at: 80000, tx: '0xb', token: '9', side: 'SELL', his_price: 0.5, src: 'data-api', lat: { src: null, obs: 80000, dec: 80020 }, decision: 'skip', reason: 'aucune position à vendre' },
  { t: 'order', ev: 'post', at: 90000, id: 2, token: '9', side: 'BUY', price: 0.5, size: 5, ahead: 0 },
  { t: 'order', ev: 'cancel', at: 95000, id: 2, reason: 'inactif' },
  { t: 'stat', at: 99000, kind: 'annulation à tort', token: '9', side: 'BUY', price: 0.5, revived_after_ms: 4000 },
  { t: 'check', at: 99500, kind: 'trade_side', ok: true },
  { t: 'check', at: 99600, ok: false },
];

test('le modèle ne garde que le dernier run et relie trades, ordres et file', () => {
  const m = createModel();
  for (const o of J) ingest(m, o);
  assert.equal(m.runs, 2);
  assert.equal(m.trades.length, 2);
  const o1 = m.orders.get(1);
  assert.equal(o1.tx, '0xa');
  assert.equal(o1.status, 'filled');
  assert.deepEqual(o1.aheadSeries.map((x) => x[1]), [100, 30]);
  assert.equal(m.orders.get(2).wrong, true);
  assert.deepEqual([m.sideChecks.ok, m.checks.bad], [1, 1]);

  const c = cascade(m, m.trades[0], 100000);
  assert.deepEqual([c.dated, c.obs, c.dec, c.post, c.end, c.endKind], [true, 1.1, 1.15, 1.2, 69.1, 'filled']);
  const c2 = cascade(m, m.trades[1], 100000);
  assert.deepEqual([c2.dated, c2.api, c2.obs, c2.post], [false, true, 0, null]);

  const k = tiles(m, windowStart(m, 'run', 100000));
  assert.equal(k.hisPct.toFixed(1), '1.0');
  assert.equal(k.myPct, -1);
  assert.equal(k.myPrice, 0.07);
  assert.equal(k.apiShare, 0.5);
  assert.deepEqual([k.posted, k.filled, k.wrong], [2, 1, 1]);
  assert.equal(performance(m, 0).length, 2);

  const mk = markets(m);
  assert.deepEqual(mk.map((x) => [x.token, x.state]), [['9', 'annulé à tort'], ['7', 'rempli']]);
});

test('une fenêtre ne remonte jamais avant le début du run', () => {
  const m = createModel();
  ingest(m, { t: 'start', at: 5_000_000 });
  assert.equal(windowStart(m, '1h', 6_000_000), 5_000_000);
  assert.equal(windowStart(m, '1h', 20_000_000), 20_000_000 - 3600e3);
});

test('ma position suit mes fills, ventes et résolution ; le prix d’entrée ne compte que les achats', () => {
  const m = createModel();
  const lat = { obs: 1, dec: 2 };
  for (const o of [
    { t: 'start', at: 1, cash: 1000 },
    { t: 'order', ev: 'post', at: 2, id: 1, token: 'a', side: 'BUY', price: 0.5, size: 10, filled: 10, kickoff: 900 },
    { t: 'trade', at: 2, tx: 'x', token: 'a', side: 'BUY', his_price: 0.5, lat, decision: 'buy', order: { id: 1 } },
    { t: 'state', at: 3, real_pos: { a: 10 }, marks: { a: 0.6 } },
    { t: 'order', ev: 'post', at: 4, id: 2, token: 'a', side: 'SELL', price: 0.54, size: 4, filled: 4 },
    { t: 'trade', at: 4, tx: 'y', token: 'a', side: 'SELL', his_price: 0.52, lat, decision: 'sell', order: { id: 2 } },
  ]) ingest(m, o);
  assert.equal(m.orders.get(1).kickoff, 900);
  // l'état date d'avant la vente : la position vient des fills
  let p = myPosition(m, 'a');
  assert.ok(Math.abs(p.shares - 6) < 1e-9 && Math.abs(p.pnl - (4 * 0.54 + 6 * 0.6 - 5)) < 1e-9 && p.avg === 0.5);
  const k = tiles(m, 0);
  assert.deepEqual([k.hisPrice, k.myPrice], [0.5, 0.5]);
  ingest(m, { t: 'resolution', at: 5, token: 'a', payout: 1 });
  p = myPosition(m, 'a');
  assert.ok(p.shares === 0 && Math.abs(p.pnl - (4 * 0.54 + 6 - 5)) < 1e-9);
  assert.equal(markets(m)[0].shares, 0);
});
