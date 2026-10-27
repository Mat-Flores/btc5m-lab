// node --test: only markets the engine bought, resolved for its token and not yet redeemed are due.
import test from 'node:test';
import assert from 'node:assert/strict';
import { due } from './redeem.mjs';

const f = (fact) => JSON.stringify({ seq: 0, fact });
const trade = (market, token, status = 'Confirmed') => f({ type: 'trade', trade: `${market}:0`, order: 'o', market, token, dir: 'buy', status, size: 5000000, price: 500000 });

test('won, lost, failed, redeemed and foreign markets', () => {
  const lines = [
    trade('0xwon', 'up'), f({ type: 'resolved', market: '0xwon', winner: 'up' }),
    trade('0xlost', 'up'), f({ type: 'resolved', market: '0xlost', winner: 'down' }),
    trade('0xfailed', 'up', 'Failed'), f({ type: 'resolved', market: '0xfailed', winner: 'up' }),
    trade('0xdone', 'up'), f({ type: 'resolved', market: '0xdone', winner: 'up' }), f({ type: 'redeemed', market: '0xdone', amount: 1, tx: 't' }),
    f({ type: 'resolved', market: '0xforeign', winner: 'up' }),
    trade('0xopen', 'up'), 'torn {',
  ];
  assert.deepEqual(due(lines), ['0xwon']);
});
