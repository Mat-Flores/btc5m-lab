// Modèle de l'UI : ingestion du journal du bot (une ligne JSON à la fois) et vues dérivées, sans DOM.
// Une ligne `start` ouvre un nouveau run : tout ce qui précède est oublié, comme le font les comptes du bot.

export function createModel() {
  return {
    runs: 0, run: null, trades: [], orders: new Map(), states: [], stats: [], warns: [], halt: null, stop: null,
    checks: { ok: 0, bad: 0 }, sideChecks: { ok: 0, bad: 0 }, feed: {}, resyncs: 0, last: 0, version: 0,
  };
}

const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);

export function ingest(m, o) {
  if (!o || typeof o !== 'object') return;
  const at = num(o.at) ?? m.last;
  m.last = Math.max(m.last, at);
  m.version++;
  switch (o.t) {
    case 'start': {
      const runs = m.runs + 1;
      Object.assign(m, createModel(), { runs, last: at, version: m.version });
      m.run = { at, mode: o.mode, wallet: o.wallet, cash: num(o.cash), tiers: o.tiers, strategy: o.strategy || null };
      break;
    }
    case 'trade': {
      const t = {
        at, tx: o.tx, token: String(o.token ?? ''), buy: o.side === 'BUY', price: num(o.his_price), usd: num(o.his_usd),
        shares: num(o.his_shares), src: o.src, lat: o.lat || {}, decision: o.decision, reason: o.reason || null,
        his: o.his_order || null, orderId: o.order ? num(o.order.id) : null, myUsd: num(o.my_usd), myShares: num(o.my_shares),
        fillShares: num(o.fill_shares), fillUsd: num(o.fill_usd), pCum: num(o.p_cum), p: num(o.p),
      };
      m.trades.push(t);
      const ord = t.orderId != null ? m.orders.get(t.orderId) : null;
      if (ord) ord.tx = t.tx;
      break;
    }
    case 'order': {
      const id = num(o.id);
      if (o.ev === 'post' || o.ev === 'rejected') {
        const ord = {
          id: id ?? -m.orders.size - 1, token: String(o.token ?? ''), buy: o.side === 'BUY', price: num(o.price), size: num(o.size) ?? 0,
          filled: 0, ahead0: num(o.ahead), ahead: num(o.ahead), aheadSeries: [], posted: at, fills: [], end: null,
          status: o.ev === 'rejected' ? 'rejected' : 'open', cause: null, wrong: false, tx: null,
        };
        if (ord.ahead != null) ord.aheadSeries.push([at, ord.ahead]);
        const now = num(o.filled) ?? 0;
        if (now > 0) { ord.filled = now; ord.fills.push({ at, shares: now, price: ord.price, tx: null }); }
        m.orders.set(ord.id, ord);
        break;
      }
      const ord = m.orders.get(id);
      if (!ord) break;
      if (o.ev === 'fill') {
        const sh = num(o.shares) ?? 0;
        ord.filled += sh;
        ord.fills.push({ at, shares: sh, price: num(o.price) ?? ord.price, tx: o.tx || null });
      } else if (o.ev === 'cancel') {
        ord.status = 'cancel'; ord.end = at; ord.cause = o.reason || null;
      } else if (o.ev === 'done' && ord.status === 'open') {
        ord.status = 'filled'; ord.end = at;
      }
      break;
    }
    case 'state': {
      const acc = (a) => (a && typeof a === 'object' ? { value: num(a.value), pnl: num(a.pnl), cash: num(a.cash), unmarked: num(a.unmarked) } : null);
      const s = { at, his: num(o.his_value), obs: acc(o.obs), dem: acc(o.dem), real: acc(o.real), pos: o.real_pos || null, marks: o.marks || null };
      m.states.push(s);
      for (const r of Array.isArray(o.orders) ? o.orders : []) {
        const ord = m.orders.get(num(r.id));
        if (ord && num(r.ahead) != null) { ord.ahead = r.ahead; ord.aheadSeries.push([at, r.ahead]); }
      }
      break;
    }
    case 'check':
      if (o.kind === 'trade_side') m.sideChecks[o.ok === true ? 'ok' : 'bad']++;
      else m.checks[o.ok === true ? 'ok' : 'bad']++;
      break;
    case 'stat': {
      m.stats.push({ at, ...o });
      if (o.kind === 'annulation à tort') {
        const key = (ord) => ord.token === String(o.token) && ord.buy === (o.side === 'BUY') && Math.abs(ord.price - o.price) < 1e-9;
        const hit = [...m.orders.values()].reverse().find((ord) => ord.status === 'cancel' && key(ord));
        if (hit) { hit.wrong = true; hit.revivedAfter = num(o.revived_after_ms); }
      }
      break;
    }
    case 'feed':
      if (typeof o.health === 'string' && o.health.startsWith('resync')) m.resyncs++;
      else m.feed[o.source] = { at, health: String(o.health) };
      break;
    case 'warn': m.warns.push({ at, what: o.what }); break;
    case 'halt': m.halt = { at, reason: o.reason }; break;
    case 'stop': m.stop = { at, reason: o.reason }; break;
    default: break;
  }
}

// ---- vues dérivées ----------------------------------------------------------------------------------------------

export const RANGES = { '1h': 3600e3, '6h': 6 * 3600e3, '1j': 24 * 3600e3, run: Infinity };

export function windowStart(m, range, now) {
  const start = m.run ? m.run.at : (m.trades[0]?.at ?? now);
  return Math.max(start, now - (RANGES[range] ?? Infinity));
}

export function quantile(xs, q) {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.max(0, Math.ceil(q * s.length) - 1))];
}

export function capital(m) {
  const s = m.states.find((x) => x.real?.value != null && x.real?.pnl != null);
  return s ? s.real.value - s.real.pnl : m.run?.cash ?? null;
}

/** Performance en % depuis le début du run : lui (valeur du portefeuille), moi réel, moi à ses prix. */
export function performance(m, from) {
  const cap = capital(m);
  const his0 = m.states.find((s) => s.his)?.his;
  return m.states.filter((s) => s.at >= from).map((s) => ({
    at: s.at,
    his: his0 && s.his ? (s.his / his0 - 1) * 100 : null,
    real: cap && s.real?.pnl != null ? (s.real.pnl / cap) * 100 : null,
    obs: cap && s.obs?.pnl != null ? (s.obs.pnl / cap) * 100 : null,
  }));
}

/** Instant où mon ordre a quitté la file : dernier fill d'un ordre rempli, sinon rien. */
function filledAt(o) { return o.status === 'filled' && o.fills.length ? o.fills[o.fills.length - 1].at : null; }

/** Les quatre comparaisons du haut. Tout ce qui n'est pas mesuré reste `null`. */
export function tiles(m, from) {
  const perf = performance(m, -Infinity);
  const last = perf[perf.length - 1] || {};
  const trades = m.trades.filter((t) => t.at >= from);
  const delays = trades.filter((t) => t.lat.src != null && t.lat.dec != null).map((t) => (t.lat.dec - t.lat.src) / 1000);
  const api = trades.filter((t) => t.src === 'data-api').length;
  // prix : mes fills comparés au prix de son fill déclencheur, pondérés par mes parts
  let his = 0, mine = 0, sh = 0;
  for (const t of trades) {
    const o = t.orderId != null ? m.orders.get(t.orderId) : null;
    const f = o ? o.fills.reduce((a, x) => [a[0] + x.shares, a[1] + x.shares * x.price], [0, 0]) : t.fillShares ? [t.fillShares, t.fillUsd] : [0, 0];
    if (f[0] > 0 && t.price != null) { sh += f[0]; mine += f[1]; his += f[0] * t.price; }
  }
  const orders = [...m.orders.values()].filter((o) => o.posted >= from);
  // en taker, un trade copié est un ordre envoyé ; il est rempli s'il a trouvé de la liquidité
  const taken = trades.filter((t) => t.orderId == null && t.fillShares != null);
  return {
    hisPct: last.his ?? null, myPct: last.real ?? null, hisValue: m.states.at(-1)?.his ?? null, myValue: m.states.at(-1)?.real?.value ?? null,
    lastMatch: trades.at(-1)?.at ?? null, delayMed: quantile(delays, 0.5), delayP90: quantile(delays, 0.9), apiShare: trades.length ? api / trades.length : null,
    hisPrice: sh ? his / sh : null, myPrice: sh ? mine / sh : null,
    hisFills: trades.length, posted: orders.length + taken.length, filled: orders.filter((o) => o.status === 'filled').length + taken.filter((t) => t.fillShares > 0).length,
    open: orders.filter((o) => o.status === 'open').length, wrong: orders.filter((o) => o.wrong).length,
  };
}

/** Un ordre ouvert de lui est supposé vivant tant que son dernier fill date de moins que le délai d'annulation. */
export function cancelAfterMs(m) { return (m.run?.strategy?.cancel_after_min ?? 360) * 60e3; }

/** Les marchés : ceux où il a agi pendant le run, avec mon état sur chacun. */
export function markets(m) {
  const by = new Map();
  for (const t of m.trades) {
    const k = by.get(t.token) || { token: t.token, last: 0, hisUsd: 0, hisFills: 0, lastPrice: null, buy: t.buy, orders: [], wrong: false, api: 0, skipped: 0, taken: 0 };
    k.last = Math.max(k.last, t.at); k.hisUsd += t.usd || 0; k.hisFills++; k.lastPrice = t.price; k.buy = t.buy;
    if (t.src === 'data-api') k.api++;
    if (t.decision === 'skip') k.skipped++;
    if (t.fillShares > 0) k.taken++; // copie prise sur le carnet (stratégie taker)
    by.set(t.token, k);
  }
  for (const o of m.orders.values()) {
    const k = by.get(o.token);
    if (!k) continue;
    k.orders.push(o);
    k.wrong ||= o.wrong;
  }
  const st = m.states.at(-1);
  return [...by.values()].map((k) => {
    const open = k.orders.filter((o) => o.status === 'open');
    const shares = num(st?.pos?.[k.token]) ?? 0;
    const mark = num(st?.marks?.[k.token]) ?? k.lastPrice;
    let state = 'observé';
    if (k.wrong) state = 'annulé à tort';
    else if (open.some((o) => o.filled > 0)) state = 'partiel';
    else if (open.length) state = open.length > 1 ? `${open.length} en file` : 'en file';
    else if (k.orders.some((o) => o.status === 'filled') || k.taken) state = 'rempli';
    else if (k.orders.some((o) => o.status === 'cancel')) state = 'annulé';
    else if (k.skipped) state = 'ignoré';
    return { ...k, open, shares, mark, myValue: shares * (mark ?? 0), state };
  }).sort((a, b) => b.last - a.last);
}

/** Segments de la cascade d'un trade, en secondes depuis son match (ou depuis l'observation quand le match n'est pas daté). */
export function cascade(m, t, now) {
  const L = t.lat;
  const t0 = L.src ?? L.obs;
  const s = (x) => (x == null || t0 == null ? null : Math.max(0, (x - t0) / 1000));
  const o = t.orderId != null ? m.orders.get(t.orderId) : null;
  const seg = { dated: L.src != null, api: t.src === 'data-api', obs: s(L.obs), dec: s(L.dec), post: o ? s(o.posted) : null, end: null, endKind: null, order: o };
  if (o) {
    if (o.status === 'filled') { seg.end = s(filledAt(o)); seg.endKind = 'filled'; }
    else if (o.status === 'cancel') { seg.end = s(o.end); seg.endKind = o.wrong ? 'wrong' : 'cancel'; }
    else if (o.status === 'open') { seg.end = s(now); seg.endKind = o.filled > 0 ? 'partial' : 'open'; }
  } else if (t.fillShares > 0) { seg.post = seg.dec; seg.end = seg.dec; seg.endKind = 'filled'; }
  return seg;
}

/** Ses ordres que l'on croit encore posés sur un token : (sens, prix) remplis depuis moins que le délai d'annulation. */
export function hisLevels(m, token, now) {
  const lim = cancelAfterMs(m);
  const lv = new Map();
  for (const t of m.trades) {
    if (t.token !== token || !t.his || now - t.at > lim) continue;
    lv.set(`${t.buy}|${t.his.price}`, { buy: t.buy, price: t.his.price, usd: t.his.usd, fills: t.his.fills, last: t.at });
  }
  return [...lv.values()];
}

/** Ma position sur un token : parts (dernier état), prix moyen d'achat (mes fills). */
export function myPosition(m, token) {
  let sh = 0, usd = 0;
  for (const o of m.orders.values()) {
    if (o.token !== token || !o.buy) continue;
    for (const f of o.fills) { sh += f.shares; usd += f.shares * f.price; }
  }
  for (const t of m.trades) if (t.token === token && t.buy && !t.orderId && t.fillShares > 0) { sh += t.fillShares; usd += t.fillUsd; }
  const st = m.states.at(-1);
  return { shares: num(st?.pos?.[token]) ?? sh, avg: sh ? usd / sh : null, mark: num(st?.marks?.[token]) };
}
