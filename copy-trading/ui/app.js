// UI de copy-trading : un seul état (journal + données publiques + choix de l'opérateur), rendu groupé par image.
// Les composants sont liés par des clés communes posées en attributs : data-token (marché), data-tx (son trade).
import { createModel, ingest, windowStart, tiles, markets, cascade, hisLevels, myPosition, performance, capital, cancelAfterMs } from './model.js';

let M = createModel();
const UI = { range: '6h', token: null, pinned: false, hover: null, filter: 'tous', conn: 'connexion', ready: false };
const EXT = { meta: new Map(), metaWait: new Set(), metaFail: new Map(), positions: new Map(), book: null, bookToken: null, history: new Map() };
const seen = new Set();
const $ = (id) => document.getElementById(id);

// ---- format ------------------------------------------------------------------------------------------------------
const nf = (d) => new Intl.NumberFormat('fr-FR', { minimumFractionDigits: d, maximumFractionDigits: d });
const N0 = nf(0), N1 = nf(1), N2 = nf(2);
const loading = () => (EXT.positions.size ? '—' : '…');
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const cents = (p) => (p == null ? '—' : `${(Math.abs(p * 100 - Math.round(p * 100)) < 1e-6 ? N0 : N1).format(p * 100)}¢`);
const usd = (x) => (x == null ? '—' : Math.abs(x) >= 1e6 ? `${N2.format(x / 1e6)} M$` : Math.abs(x) >= 1000 ? `${N0.format(x)} $` : `${N2.format(x)} $`);
const sgn = (x, f) => (x == null ? '—' : `${x > 0 ? '+' : x < 0 ? '−' : ''}${f(Math.abs(x))}`);
const pct = (x) => sgn(x, (v) => `${N2.format(v)} %`);
const shares = (x) => (x == null ? '—' : x >= 1000 || x === 0 ? N0.format(x) : N2.format(x));
const hms = (ms) => new Date(ms).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
const hm = (ms) => new Date(ms).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
function dur(s) {
  if (s == null) return '—';
  if (s < 10) return `${N1.format(s)} s`;
  if (s < 60) return `${N0.format(s)} s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  const h = Math.floor(s / 3600), mn = Math.round((s % 3600) / 60);
  return mn ? `${h} h ${String(mn).padStart(2, '0')}` : `${h} h`;
}
const shortAddr = (a) => (a ? `${a.slice(0, 6)}…${a.slice(-4)}` : '—');
const now = () => Math.max(Date.now(), M.last);

// ---- rendu groupé -------------------------------------------------------------------------------------------------
let queued = false;
function invalidate() { if (!queued) { queued = true; requestAnimationFrame(render); } }
const last = new Map();
function put(id, html) { if (last.get(id) !== html) { last.set(id, html); $(id).innerHTML = html; } }

function render() {
  queued = false;
  // un clic en cours ne doit pas voir sa cible remplacée : on attend le relâchement
  if (UI.pressing) return;
  // les échelles avancent à la minute : un panneau n'est redessiné que si ses données changent
  const live = now(), t = Math.ceil(live / 60e3) * 60e3, from = windowStart(M, UI.range, t);
  const mks = markets(M);
  if (!UI.pinned || !mks.some((k) => k.token === UI.token)) {
    // sans choix de l'opérateur, on suit le dernier marché où j'ai un ordre, sinon son dernier trade
    UI.token = (mks.find((k) => k.orders.length) || mks[0])?.token ?? null;
  }
  nav(live);
  perf(from, t);
  topTiles(from, t);
  list(mks);
  marketHead(mks);
  marketChart(from, t);
  waterfall(from, t);
  side(t);
  applyHover();
  for (const tok of mks.map((k) => k.token)) wantMeta(tok);
  flushMeta();
}

// ---- barre --------------------------------------------------------------------------------------------------------
function nav(t) {
  const r = M.run, s = r?.strategy;
  const strat = s ? `${s.exec === 'taker' ? 'Taker' : 'Maker'} · ${s.buy === 'first_fill' ? '1re entrée' : 'cumul'} · ${s.sell === 'all' ? 'tout' : 'lots'} · ${Math.round(s.cancel_after_min / 60)} h`
    : M.trades.some((x) => x.orderId != null) ? 'Maker' : M.trades.some((x) => x.fillShares != null) ? 'Taker' : 'Stratégie —';
  const fd = (name, label) => {
    const f = M.feed[name];
    const cls = !f || stale ? 'off' : f.health === 'Valid' ? '' : 'bad';
    return `<span class="dot ${cls}" title="${esc(f ? f.health : 'aucune nouvelle')}"></span>${label}`;
  };
  const lat = tiles(M, windowStart(M, '1h', t)).delayMed;
  const age = M.last ? Math.max(0, Math.round((Date.now() - M.last) / 1000)) : null;
  // plus de 2 min sans ligne de journal (le bot en écrit une par minute) : la santé affichée n'est plus actuelle
  const stale = age != null && age > 120;
  const connBad = UI.conn !== 'ok';
  put('nav', `
    <div class="logo"><svg width="28" height="28" viewBox="0 0 28 28" aria-hidden="true"><rect width="28" height="28" rx="7" fill="#3262FF"/><path d="M7 18 L12 12 L16 15 L21 9" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/><circle cx="21" cy="9" r="2" fill="#FF9900"/></svg>Copy-trading</div>
    ${r?.wallet ? `<a class="chip" href="https://polymarket.com/profile/${esc(r.wallet)}" target="_blank" rel="noopener"><span class="av"></span><b>${esc(shortAddr(r.wallet))}</b> suivi</a>` : ''}
    ${r ? `<span class="chip ${r.mode === 'live' ? 'live' : 'paper'}">${r.mode === 'live' ? 'Live' : 'Paper'} · ${usd(r.cash)}</span>` : ''}
    <span class="chip">${esc(strat)}</span>
    <span class="sp"></span>
    ${connBad ? `<span class="chip alert"><span class="dot bad"></span>Journal ${esc(UI.conn)}</span>` : ''}
    ${M.halt ? `<span class="chip alert" title="${esc(M.halt.reason)}">Bot arrêté</span>` : ''}
    <span class="chip">${fd('chain', 'Chaîne')} <b>${lat == null ? '' : dur(lat)}</b> · ${fd('market', 'Market')} · <b>${M.checks.ok}</b> contrôles${M.checks.bad ? ` · <span class="dn">${M.checks.bad} écarts</span>` : ''}${M.resyncs ? ` · ${M.resyncs} resync` : ''}</span>
    <div class="tabs" role="group" aria-label="Période">${['1h', '6h', '1j', 'run'].map((k) => `<button data-range="${k}" aria-pressed="${UI.range === k}">${k === 'run' ? 'Run' : k.toUpperCase()}</button>`).join('')}</div>
    ${stale ? `<span class="chip alert"><span class="dot bad"></span>Journal muet depuis ${dur(age)}</span>` : `<span class="clock">${age == null ? 'en attente' : age < 2 ? 'à l’instant' : `il y a ${dur(age)}`}</span>`}`);
}

// ---- performance ---------------------------------------------------------------------------------------------------
function scaleX(from, to, x0, x1) { const k = (x1 - x0) / Math.max(1, to - from); return (t) => x0 + (t - from) * k; }
function niceTicks(lo, hi, n) {
  const step0 = (hi - lo) / n, mag = 10 ** Math.floor(Math.log10(step0 || 1));
  const step = [1, 2, 2.5, 5, 10].map((k) => k * mag).find((s) => s >= step0) || mag;
  const out = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) out.push(+v.toFixed(6));
  return out;
}
function perf(from, t) {
  const el = $('perf'), w = Math.max(300, el.clientWidth - 2), h = 178;
  const pts = performance(M, from);
  const head = `<div class="hd"><h2>Performance</h2><span class="sub">en % du capital</span><div class="lg"><span><i style="background:var(--him)"></i>Lui</span><span><i style="background:var(--me)"></i>Moi (réel)</span><span><i style="background:var(--t3)"></i>Moi à ses prix</span></div></div>`;
  if (pts.length < 2) return put('perf', `${head}<div class="empty">Courbes disponibles après deux relevés (un par minute).</div>`);
  const vals = pts.flatMap((p) => [p.his, p.real, p.obs]).filter((v) => v != null);
  let lo = Math.min(0, ...vals), hi = Math.max(0, ...vals);
  const pad = Math.max(0.1, (hi - lo) * 0.15); lo -= pad; hi += pad;
  const X = scaleX(from, t, 14, w - 52), Y = (v) => 14 + (hi - v) / (hi - lo) * (h - 40);
  const line = (k) => pts.filter((p) => p[k] != null).map((p) => `${X(p.at).toFixed(1)},${Y(p[k]).toFixed(1)}`).join(' ');
  const both = pts.filter((p) => p.his != null && p.real != null);
  const gap = both.length > 1 ? `<path d="M${both.map((p) => `${X(p.at).toFixed(1)} ${Y(p.his).toFixed(1)}`).join(' L')} L${[...both].reverse().map((p) => `${X(p.at).toFixed(1)} ${Y(p.real).toFixed(1)}`).join(' L')} Z" fill="${both.at(-1).real < both.at(-1).his ? 'rgba(226,57,57,.10)' : 'rgba(66,199,114,.10)'}"/>` : '';
  const ticks = niceTicks(lo, hi, 3).map((v) => `<line x1="14" x2="${w - 52}" y1="${Y(v)}" y2="${Y(v)}" stroke="#2D3037"/><text x="${w - 46}" y="${Y(v) + 4}" font-size="11" fill="#9AA0A9">${v > 0 ? '+' : ''}${N1.format(v)} %</text>`).join('');
  const times = [0, 1, 2, 3].map((i) => from + (t - from) * i / 3).map((x, i) => `<text x="${X(x)}" y="${h - 6}" font-size="11" fill="#9AA0A9" text-anchor="${i === 0 ? 'start' : i === 3 ? 'end' : 'middle'}">${hm(x)}</text>`).join('');
  // les coupures du flux chaîne, là où la copie a perdu sa source rapide
  const cut = M.trades.filter((x) => x.src === 'data-api' && x.at >= from).slice(0, 1).map((x) => `<line x1="${X(x.at)}" x2="${X(x.at)}" y1="14" y2="${h - 26}" stroke="#FF7A7A" stroke-dasharray="2 3"/><text x="${X(x.at) + 6}" y="24" font-size="11" fill="#FF7A7A">${hm(x.at)} secours API</text>`).join('');
  const end = (k, c) => { const p = [...pts].reverse().find((q) => q[k] != null); return p ? `<circle cx="${X(p.at)}" cy="${Y(p[k])}" r="4" fill="${c}"/>` : ''; };
  put('perf', `${head}<svg class="chart" width="${w}" height="${h}" role="img" aria-label="Performance lui et moi">${ticks}${gap}${cut}
    <polyline fill="none" stroke="#9AA0A9" stroke-width="1.5" stroke-dasharray="4 4" points="${line('obs')}"/>
    <polyline fill="none" stroke="#FF9900" stroke-width="2.5" points="${line('his')}"/>
    <polyline fill="none" stroke="#3262FF" stroke-width="2.5" points="${line('real')}"/>${end('his', '#FF9900')}${end('real', '#3262FF')}${times}</svg>`);
}

// ---- tuiles --------------------------------------------------------------------------------------------------------
function tile(id, title, his, me, delta) {
  const v = (x) => (x[0] == null || x[0] === '—' ? '<span class="big null">NULL</span>' : `<span class="big ${x[2] || ''}">${x[0]}</span>`);
  put(id, `<h3>${title}</h3>
    <div class="vr"><span class="who h">Lui</span>${v(his)}<span class="small">${his[1] || ''}</span></div>
    <div class="vr"><span class="who m">Moi</span>${v(me)}<span class="small">${me[1] || ''}</span></div>
    ${delta ? `<span class="delta ${delta[1]}">${delta[0]}</span>` : ''}`);
}
function topTiles(from) {
  const k = tiles(M, from);
  const d = k.hisPct != null && k.myPct != null ? k.myPct - k.hisPct : null;
  tile('t-ret', 'Rendement', [pct(k.hisPct), usd(k.hisValue)], [pct(k.myPct), usd(k.myValue), k.myPct < 0 ? 'dn' : k.myPct > 0 ? 'up' : ''],
    d == null ? null : [`${sgn(d, (v) => N2.format(v))} pt d’écart`, d < 0 ? 'bad' : 'good']);
  tile('t-delay', 'Délai de copie', [k.lastMatch ? hms(k.lastMatch) : null, 'dernier match'], [k.delayMed == null ? null : `+${dur(k.delayMed)}`, k.delayP90 == null ? '' : `P90 ${dur(k.delayP90)}`],
    k.apiShare == null ? null : k.apiShare > 0 ? [`${N0.format(k.apiShare * 100)} % via secours API`, 'bad'] : ['100 % via la chaîne', 'good']);
  const dp = k.hisPrice != null ? (k.myPrice - k.hisPrice) * 100 : null;
  tile('t-price', 'Prix d’entrée', [k.hisPrice == null ? null : cents(k.hisPrice), 'copiés'], [k.myPrice == null ? null : cents(k.myPrice), 'mes fills'],
    dp == null ? null : [`${sgn(dp, (v) => N1.format(v))}¢ d’écart`, dp > 0.05 ? 'bad' : 'good']);
  tile('t-exec', 'Exécution', [N0.format(k.hisFills), 'fills'], [`${k.filled} / ${k.posted}`, 'ordres remplis'],
    [`${k.open} en file${k.wrong ? ` · ${k.wrong} annulé${k.wrong > 1 ? 's' : ''} à tort` : ''}`, k.wrong ? 'bad' : 'neu']);
}

// ---- marchés -------------------------------------------------------------------------------------------------------
const FILTERS = { tous: 'Tous', ouverts: 'Ouverts', positions: 'Positions', ecarts: 'Écarts' };
function icon(tok, cls = 'ic') {
  const m = EXT.meta.get(tok);
  return m?.icon ? `<img class="${cls}" src="${esc(m.icon)}" alt="">` : `<span class="${cls}">${esc((m?.title || '?').slice(0, 2))}</span>`;
}
function outcomePill(tok, price) {
  const o = EXT.meta.get(tok)?.outcome;
  const cls = o === 'Yes' ? 'yes' : o === 'No' ? 'no' : 'unk';
  const label = o === 'Yes' ? 'Oui' : o === 'No' ? 'Non' : o || 'Issue';
  return `<span class="pill ${cls}">${esc(label)} ${cents(price)}</span>`;
}
function list(mks) {
  const cap = capital(M) || 1;
  const hisVal = M.states.at(-1)?.his || 0;
  const hisPos = (tok) => EXT.positions.get(tok)?.currentValue ?? 0;
  const pass = { tous: () => true, ouverts: (k) => k.open.length > 0, positions: (k) => k.shares > 0, ecarts: (k) => k.wrong || k.api > 0 }[UI.filter];
  const shown = mks.filter(pass);
  const maxHis = Math.max(1e-9, ...mks.map((k) => hisPos(k.token) / (hisVal || 1)));
  const maxMe = Math.max(1e-9, ...mks.map((k) => k.myValue / cap));
  put('mk-count', `${mks.length}`);
  put('filters', Object.entries(FILTERS).map(([k, v]) => `<button data-filter="${k}" aria-pressed="${UI.filter === k}">${v}</button>`).join(''));
  if (!shown.length) return put('rows', `<div class="empty">${mks.length ? 'Aucun marché pour ce filtre.' : M.run ? 'Il n’a encore rien fait depuis le début du run.' : 'En attente du journal…'}</div>`);
  put('rows', shown.map((k) => {
    const meta = EXT.meta.get(k.token);
    const hw = (hisPos(k.token) / (hisVal || 1)) / maxHis * 100, mw = (k.myValue / cap) / maxMe * 100;
    const bad = k.state.includes('tort');
    return `<button class="mk" role="option" data-token="${esc(k.token)}" aria-selected="${k.token === UI.token}">
      ${icon(k.token)}
      <div style="min-width:0"><div class="q">${esc(meta?.title || `Marché ${k.token.slice(0, 6)}…${k.token.slice(-4)}`)}</div>
        <div class="bars" title="Sa part de son portefeuille / ma part du mien"><div class="bar"><b style="width:${hw.toFixed(1)}%;background:var(--him)"></b></div><div class="bar"><b style="width:${mw.toFixed(1)}%;background:var(--me)"></b></div></div></div>
      <div class="st">${outcomePill(k.token, k.mark)}<small class="${bad ? 'dn' : ''}">${esc(k.state)}</small></div></button>`;
  }).join(''));
}

// ---- marché sélectionné --------------------------------------------------------------------------------------------
function marketHead(mks) {
  const k = mks.find((x) => x.token === UI.token);
  if (!k) return put('mhead', `<div class="empty"><b>Aucun marché.</b> Dès qu’il trade, son marché s’affiche ici avec mes ordres.</div>`);
  const m = EXT.meta.get(k.token);
  const end = m?.end ? new Date(m.end).toLocaleDateString('fr-FR', { day: 'numeric', month: 'short', year: 'numeric' }) : '—';
  put('mhead', `<div class="mh">${icon(k.token)}<div style="min-width:0"><h2 class="q">${esc(m?.question || `Marché ${k.token.slice(0, 6)}…${k.token.slice(-4)}`)}</h2>
    <span class="small">Issue suivie ${outcomePill(k.token, k.mark)} · fin le ${esc(end)} · ${k.hisFills} fills de lui · ${k.orders.length} ordre${k.orders.length > 1 ? 's' : ''} de moi</span></div>
    ${UI.pinned ? '<button class="chip follow" data-follow title="Revenir au dernier marché actif">Suivre le direct</button>' : ''}
    <div class="pct"><b>${k.mark == null ? '—' : `${N0.format(k.mark * 100)} %`}</b><div class="small">de chance</div></div></div>`);
}

function marketChart(from, t) {
  const tok = UI.token, el = $('mchart');
  if (!tok) return put('mchart', '');
  wantHistory(tok, from);
  const w = Math.max(400, el.clientWidth), h = 318, X = scaleX(from, t, 14, w - 56);
  const hist = (EXT.history.get(tok)?.points || []).filter((p) => p.t * 1000 >= from);
  const fills = M.trades.filter((x) => x.token === tok && x.at >= from && x.price != null);
  const orders = [...M.orders.values()].filter((o) => o.token === tok && (o.end ?? t) >= from);
  const prices = [...hist.map((p) => p.p), ...fills.map((f) => f.price), ...orders.map((o) => o.price)].filter((p) => p != null);
  if (!prices.length) return put('mchart', `<div class="empty">Pas encore de prix sur la période pour ce marché.</div>`);
  let lo = Math.min(...prices) * 100, hi = Math.max(...prices) * 100;
  const pad = Math.max(0.6, (hi - lo) * 0.2); lo = Math.max(0, lo - pad); hi = Math.min(100, hi + pad);
  const top = 16, ph = 200, Y = (p) => top + (hi - p * 100) / (hi - lo) * ph;
  const grid = niceTicks(lo, hi, 4).map((v) => `<line x1="14" x2="${w - 56}" y1="${Y(v / 100)}" y2="${Y(v / 100)}" stroke="#2D3037"/><text x="${w - 50}" y="${Y(v / 100) + 4}" font-size="11" fill="#9AA0A9">${cents(v / 100)}</text>`).join('');
  const hl = hist.length > 1 ? `<polyline fill="none" stroke="#9AA0A9" stroke-width="1.5" points="${hist.map((p) => `${X(p.t * 1000).toFixed(1)},${Y(p.p).toFixed(1)}`).join(' ')}"/>` : '';
  const bk = EXT.bookToken === tok && EXT.book && !EXT.book.error ? EXT.book : null;
  const bestBid = bk ? Math.max(...(bk.bids || []).map((l) => +l.price)) : null, bestAsk = bk ? Math.min(...(bk.asks || []).map((l) => +l.price)) : null;
  const band = bestBid > 0 && bestAsk < 1 && bestAsk > bestBid && bestAsk * 100 <= hi && bestBid * 100 >= lo
    ? `<rect x="14" y="${Y(bestAsk)}" width="${w - 70}" height="${Y(bestBid) - Y(bestAsk)}" fill="rgba(255,255,255,.05)"/><text x="${w - 62}" y="${Y(bestAsk) - 5}" font-size="11" fill="#9AA0A9" text-anchor="end">carnet maintenant : ${cents(bestBid)} achat / ${cents(bestAsk)} vente</text>` : '';
  const lastOrd = orders.at(-1);
  const ord = orders.map((o) => {
    const x0 = X(Math.max(o.posted, from)), x1 = X(o.end ?? t), y = Y(o.price) - 3;
    const open = o.status === 'open', cancel = o.status === 'cancel';
    const fl = o.fills.map((f) => `<circle cx="${X(f.at)}" cy="${y}" r="4" fill="#3262FF" stroke="#0C0E13" stroke-width="1.5"><title>Mon fill ${shares(f.shares)} à ${cents(f.price)} · ${hms(f.at)}</title></circle>`).join('');
    const endMark = cancel ? `<path d="M${x1 - 4} ${y - 4} L${x1 + 4} ${y + 4} M${x1 + 4} ${y - 4} L${x1 - 4} ${y + 4}" stroke="${o.wrong ? '#E23939' : '#9AA0A9'}" stroke-width="2"/>` : '';
    const trig = o.tx ? M.trades.find((x) => x.tx === o.tx) : null;
    const delay = trig?.lat?.src != null ? ` · +${dur((o.posted - trig.lat.src) / 1000)}` : '';
    return `<g${o.tx ? ` data-tx="${esc(o.tx)}"` : ''} class="pt"><line x1="${x0}" x2="${x1}" y1="${y}" y2="${y}" stroke="#5485FF" stroke-width="${open ? 2.5 : 3}" ${open || cancel ? 'stroke-dasharray="5 4"' : ''} opacity="${cancel ? 0.55 : 1}"/>
      ${o.posted >= from ? `<circle cx="${x0}" cy="${y}" r="5" fill="#0C0E13" stroke="#5485FF" stroke-width="2.5"/>` : ''}${fl}${endMark}
      ${o === lastOrd ? `<text x="${Math.min(x0 + 8, w - 230)}" y="${y - 10}" font-size="11" font-weight="600" fill="#5485FF">${o.posted >= from ? `posé ${hm(o.posted)}${delay}` : 'mon ordre'} · ${shares(o.filled)} / ${shares(o.size)} rempli</text>` : ''}
      <title>Mon ordre ${o.buy ? 'd’achat' : 'de vente'} ${cents(o.price)} × ${shares(o.size)} · posé ${hms(o.posted)}${delay} · ${o.status === 'filled' ? 'rempli' : cancel ? `annulé (${o.cause || ''})${o.wrong ? ' à tort' : ''}` : `${shares(o.filled)} rempli, ${shares(o.ahead)} devant`}</title></g>`;
  }).join('');
  const maxUsd = Math.max(1, ...fills.map((f) => f.usd || 0));
  const dots = fills.map((f) => `<circle class="pt" data-tx="${esc(f.tx)}" cx="${X(f.at).toFixed(1)}" cy="${Y(f.price).toFixed(1)}" r="${(3 + 6 * Math.sqrt((f.usd || 0) / maxUsd)).toFixed(1)}" fill="${f.buy ? '#FF9900' : '#FFB547'}" stroke="#0C0E13" stroke-width="1.5"><title>Lui : ${f.buy ? 'achat' : 'vente'} ${cents(f.price)} · ${usd(f.usd)} · ${hms(f.at)}</title></circle>`).join('');
  // mes copies prises sur le carnet (taker) : points bleus, comme mes fills d'ordres posés
  const taken = fills.filter((f) => f.fillShares > 0 && f.orderId == null).map((f) => `<circle class="pt" data-tx="${esc(f.tx)}" cx="${X(f.at).toFixed(1)}" cy="${Y(f.fillUsd / f.fillShares).toFixed(1)}" r="5" fill="#3262FF" stroke="#0C0E13" stroke-width="1.5"><title>Mon fill ${shares(f.fillShares)} à ${cents(f.fillUsd / f.fillShares)} · ${hms(f.at)}</title></circle>`).join('');
  // la file devant mes ordres, relevée chaque minute
  const qs = orders.filter((o) => o.aheadSeries.length);
  const qmax = Math.max(1, ...qs.flatMap((o) => o.aheadSeries.map((a) => a[1])));
  const qy = (v) => 268 + (1 - v / qmax) * 36;
  const queue = qs.map((o) => `<polyline fill="none" stroke="#5485FF" stroke-width="2" ${o.status === 'open' ? 'stroke-dasharray="5 4"' : ''} points="${o.aheadSeries.filter((a) => a[0] >= from).map((a) => `${X(a[0]).toFixed(1)},${qy(a[1]).toFixed(1)}`).join(' ')}"/>`).join('');
  const openO = qs.filter((o) => o.status === 'open').at(-1);
  const qlabel = openO ? `<text x="${w - 56}" y="312" font-size="11" fill="#B6BBC4" text-anchor="end">${shares(openO.ahead)} parts devant · −${shares(Math.max(0, (openO.ahead0 ?? 0) - (openO.ahead ?? 0)))} depuis la pose</text>` : '';
  const times = [0, 1, 2, 3].map((i) => from + (t - from) * i / 3).map((x, i) => `<text x="${X(x)}" y="${top + ph + 16}" font-size="11" fill="#9AA0A9" text-anchor="${i === 0 ? 'start' : i === 3 ? 'end' : 'middle'}">${hm(x)}</text>`).join('');
  put('mchart', `<svg class="chart" width="${w}" height="${h}" role="img" aria-label="Prix du marché, ses fills et mes ordres">${grid}${band}${hl}${dots}${ord}${taken}${times}
    <line x1="14" x2="${w - 56}" y1="${top + ph + 28}" y2="${top + ph + 28}" stroke="#2D3037"/><text x="14" y="${top + ph + 46}" font-size="11" fill="#9AA0A9">File devant mes ordres</text>${qs.length ? queue : `<text x="14" y="${top + ph + 64}" font-size="11" fill="#9AA0A9">aucun ordre posé ici</text>`}${qlabel}</svg>`);
}

// ---- cascade des décalages -----------------------------------------------------------------------------------------
const MAXS = 21600;
function logX(s, W) { return W * (Math.log10(Math.min(MAXS, Math.max(0.1, s))) + 1) / (Math.log10(MAXS) + 1); }
function myText(t, c) {
  const o = c.order;
  if (t.decision === 'skip') return [`Ignoré · ${t.reason || ''}`, 'small'];
  if (!o) {
    if (t.fillShares > 0) return [`Rempli ${cents(t.fillUsd / t.fillShares)} × ${shares(t.fillShares)} (taker)`, ''];
    if (t.decision === 'sell' || t.decision === 'buy') return [t.reason || 'Rien d’exécuté', 'small'];
    return ['—', 'small'];
  }
  const px = `${cents(o.price)} × ${shares(o.size)}`;
  if (o.status === 'filled') return [`Rempli ${px} · ${dur(c.end - (c.post ?? 0))}`, ''];
  if (o.status === 'cancel') return o.wrong ? [`Annulé à tort · relancé ${dur((o.revivedAfter ?? 0) / 1000)} après`, 'dn'] : [`Annulé (${o.cause || ''}) · ${shares(o.filled)} rempli`, 'small'];
  if (o.status === 'rejected') return ['Refusé par le CLOB', 'dn'];
  return [`${o.filled > 0 ? `Partiel ${shares(o.filled)} /` : 'Posé'} ${px} · file ${shares(o.ahead)}`, ''];
}
function waterfall(from, t) {
  const wb = $('wbody');
  // largeur de la colonne « décalage » : la grille de .wr (70 px | 27 % | reste | 30 %) moins le rembourrage
  const inner = wb.clientWidth - 28;
  const W = Math.max(160, inner - 70 - Math.max(205, inner * 0.27) - Math.max(210, inner * 0.3) - 10);
  const ticks = [[0.1, '0'], [1, '1 s'], [10, '10 s'], [60, '1 min'], [3600, '1 h'], [21600, '6 h']];
  put('whead', `<span>Heure</span><span>Lui</span><span title="Échelle logarithmique : son match → ma détection → mon ordre → mon fill">Décalage <span class="small">(log)</span><div class="axis">${ticks.map(([s, l]) => `<span style="left:${logX(s, W).toFixed(0)}px">${l}</span>`).join('')}</div></span><span style="padding-left:14px">Moi</span>`);
  const rows = M.trades.filter((x) => x.at >= from).slice(-80).reverse();
  if (!rows.length) return put('wbody', `<div class="empty">Aucun trade de lui sur la période. ${UI.range !== 'run' ? 'Élargis à <b>Run</b> pour tout voir.' : ''}</div>`);
  put('wbody', rows.map((x) => {
    const c = cascade(M, x, t);
    const seg = [];
    if (!c.dated) seg.push(`<line x1="0" x2="${logX(1, W)}" y1="7" y2="7" stroke="#9AA0A9" stroke-dasharray="1 3"><title>Match non daté (1er trade sur ce token)</title></line>`);
    else seg.push(`<rect x="0" y="5" width="${Math.max(2, logX(c.obs ?? 0.1, W)).toFixed(1)}" height="4" rx="2" fill="${c.api ? '#E23939' : '#FF9900'}"><title>Son match → ma détection : ${dur(c.obs)}${c.api ? ' (secours API)' : ''}</title></rect>`);
    if (c.dec != null && c.obs != null) seg.push(`<rect x="${logX(c.obs, W).toFixed(1)}" y="5" width="${Math.max(2, logX(c.dec, W) - logX(c.obs, W)).toFixed(1)}" height="4" fill="#9AA0A9"><title>Décision : ${dur(c.dec - c.obs)}</title></rect>`);
    if (c.post != null && c.end != null) {
      const x0 = logX(c.post, W), x1 = logX(c.end, W);
      const k = c.endKind;
      const col = k === 'wrong' ? '#E23939' : '#5485FF';
      seg.push(`<line x1="${x0.toFixed(1)}" x2="${x1.toFixed(1)}" y1="7" y2="7" stroke="${k === 'filled' ? '#3262FF' : col}" stroke-width="3" ${k === 'filled' ? '' : 'stroke-dasharray="4 3"'} opacity="${k === 'cancel' ? 0.5 : 1}"><title>Mon ordre → ${k === 'filled' ? 'rempli' : k === 'open' ? 'en file' : k === 'partial' ? 'partiellement rempli' : 'annulé'} : ${dur(c.end - c.post)}</title></line>`);
      if (k === 'filled') seg.push(`<circle cx="${x1.toFixed(1)}" cy="7" r="4" fill="#3262FF"/>`);
      else if (k === 'open' || k === 'partial') seg.push(`<circle cx="${x1.toFixed(1)}" cy="7" r="4" fill="#0C0E13" stroke="#5485FF" stroke-width="2"/>`);
      else seg.push(`<path d="M${x1 - 4} 3 L${x1 + 4} 11 M${x1 + 4} 3 L${x1 - 4} 11" stroke="${k === 'wrong' ? '#E23939' : '#9AA0A9'}" stroke-width="2"/>`);
    }
    const [txt, cls] = myText(x, c);
    const fresh = UI.ready && !seen.has(x.tx);
    seen.add(x.tx);
    return `<button class="wr${fresh ? ' new' : ''}" data-tx="${esc(x.tx)}" data-token="${esc(x.token)}">
      <span class="t">${hms(x.at)}</span>
      <span class="a">${icon(x.token)}<b class="${x.buy ? '' : 'dn'}">${x.buy ? 'Achat' : 'Vente'}</b> ${esc(EXT.meta.get(x.token)?.outcome === 'Yes' ? 'Oui' : EXT.meta.get(x.token)?.outcome === 'No' ? 'Non' : '')} ${cents(x.price)} · ${usd(x.usd)}</span>
      <svg width="${W}" height="14" aria-hidden="true">${seg.join('')}</svg>
      <span class="o ${cls}" title="${esc(txt)}">${esc(txt)}</span></button>`;
  }).join(''));
}

// ---- détail du marché (droite) -------------------------------------------------------------------------------------
function side(t) {
  const tok = UI.token;
  if (!tok) { put('cmp', ''); put('book', `<div class="empty">Choisis un marché.</div>`); return; }
  wantBook(tok);
  const his = EXT.positions.get(tok), me = myPosition(M, tok);
  const meVal = me.mark != null && me.avg != null ? me.shares * (me.mark - me.avg) : null;
  const hisSum = (k) => { const b = M.trades.filter((x) => x.token === k && x.buy); const v = b.reduce((a, x) => a + (x.usd || 0), 0); const hv = M.states.at(-1)?.his; return b.length ? `Ses achats ici pendant le run : ${usd(v)} en ${b.length} fills${hv ? ` (${N2.format(v / hv * 100)} % de son portefeuille)` : ''}.` : 'Pas d’achat de lui ici pendant le run.'; };
  put('cmp', `<h3>Position sur ce marché</h3>
    <div class="cr" style="border-top:0;color:var(--t3);font-size:12px"><span></span><span>Parts</span><span>Prix moyen</span><span>PnL</span></div>
    <div class="cr"><span class="who h">Lui</span><span>${his ? shares(his.size) : loading()}</span><span>${his ? cents(his.avgPrice) : loading()}</span><span class="${!his ? '' : his.cashPnl < 0 ? 'dn' : his.cashPnl > 0 ? 'up' : ''}">${his ? sgn(his.cashPnl, usd) : loading()}</span></div>
    <div class="cr"><span class="who m">Moi</span><span>${shares(me.shares)}</span><span>${cents(me.avg)}</span><span class="${meVal < -0.005 ? 'dn' : meVal > 0.005 ? 'up' : ''}">${meVal == null ? '—' : sgn(meVal, usd)}</span></div>
    <div class="small" style="margin-top:8px">${hisSum(tok)}${his && me.avg != null && his.avgPrice ? ` Écart de prix moyen : ${sgn((me.avg - his.avgPrice) * 100, (v) => N1.format(v))}¢.` : ''}</div>`);
  const b = EXT.bookToken === tok ? EXT.book : null;
  const mine = [...M.orders.values()].filter((o) => o.token === tok && o.status === 'open');
  const hisL = hisLevels(M, tok, t);
  const tagFor = (p, buySide) => {
    const h = hisL.some((l) => l.buy === buySide && Math.abs(l.price - p) < 1e-9) ? '<span class="tag h">Lui</span>' : '';
    const m = mine.some((o) => o.buy === buySide && Math.abs(o.price - p) < 1e-9) ? '<span class="tag m">Moi</span>' : '';
    return [h + m, !!m];
  };
  let bookHtml = '<div class="small">Carnet en chargement…</div>';
  if (b?.error) bookHtml = `<div class="small">Carnet indisponible (${esc(b.error)}).</div>`;
  else if (b) {
    const asks = (b.asks || []).map((l) => [+l.price, +l.size]).sort((x, y) => x[0] - y[0]).slice(0, 4).reverse();
    const bids = (b.bids || []).map((l) => [+l.price, +l.size]).sort((x, y) => y[0] - x[0]).slice(0, 4);
    const mx = Math.max(1, ...asks.map((l) => l[1]), ...bids.map((l) => l[1]));
    const row = (l, isBid) => { const [tag, my] = tagFor(l[0], isBid); return `<div class="lv${my ? ' mine' : ''}"><i class="dp" style="width:${(l[1] / mx * 100).toFixed(0)}%;background:${isBid ? 'rgba(66,199,114,.12)' : 'rgba(226,57,57,.12)'}"></i><span class="${isBid ? 'up' : 'dn'}">${cents(l[0])}</span><span>${shares(l[1])}${tag}</span><span>${usd(l[0] * l[1])}</span></div>`; };
    const spread = asks.length && bids.length ? cents(asks.at(-1)[0] - bids[0][0]) : '—';
    bookHtml = `<div class="lv hdr"><span>Prix</span><span>Parts</span><span>Total</span></div>${asks.map((l) => row(l, false)).join('')}
      <div class="lv spread"><span>Écart</span><span>${spread}</span><span></span></div>${bids.map((l) => row(l, true)).join('')}`;
  }
  const outcome = EXT.meta.get(tok)?.outcome;
  const queue = mine.slice(-2).map((o) => {
    const done = Math.max(0, (o.ahead0 ?? 0) - (o.ahead ?? 0)), tot = Math.max(1, o.ahead0 ?? 0);
    const lastHis = [...M.trades].reverse().find((x) => x.token === tok && x.buy === o.buy && x.his && Math.abs(x.his.price - o.price) < 1e-9)?.at ?? o.posted;
    return `<h3 style="margin-top:16px">Ma place dans la file à ${cents(o.price)}</h3>
      <div class="qb" role="progressbar" aria-valuemin="0" aria-valuemax="${tot}" aria-valuenow="${done}"><b style="width:${(done / tot * 100).toFixed(1)}%"></b></div>
      <div class="qrow small"><span>${shares(done)} parts passées devant moi</span><span>${shares(o.ahead)} restantes</span></div>
      <div class="small" style="margin-top:8px">Posé à ${hms(o.posted)} · ${shares(o.filled)} / ${shares(o.size)} rempli · annulation auto si plus aucun fill de lui avant ${hm(lastHis + cancelAfterMs(M))}</div>`;
  }).join('');
  put('book', `<h3>Carnet${outcome ? ` · ${esc(outcome === 'Yes' ? 'Oui' : outcome === 'No' ? 'Non' : outcome)}` : ''}</h3>${bookHtml}${queue || '<h3 style="margin-top:16px">Ma place dans la file</h3><div class="small">Aucun ordre ouvert de moi sur ce marché.</div>'}`);
}

// ---- associations : survol, clic, clavier --------------------------------------------------------------------------
function applyHover() {
  document.querySelectorAll('.hl').forEach((e) => e.classList.remove('hl'));
  const h = UI.hover;
  for (const id of ['wbody', 'mchart']) $(id).classList.toggle('dimming', !!h?.tx);
  if (!h) return;
  if (h.tx) document.querySelectorAll(`[data-tx="${CSS.escape(h.tx)}"]`).forEach((e) => e.classList.add('hl'));
  if (h.token) document.querySelectorAll(`#rows [data-token="${CSS.escape(h.token)}"]`).forEach((e) => e.classList.add('hl'));
}
function hoverFrom(el) {
  const tx = el?.closest('[data-tx]')?.dataset.tx || null;
  const token = el?.closest('[data-token]')?.dataset.token || (tx ? M.trades.find((x) => x.tx === tx)?.token : null) || null;
  return tx || token ? { tx, token } : null;
}
document.addEventListener('pointerdown', () => { UI.pressing = true; });
document.addEventListener('pointerup', () => { UI.pressing = false; setTimeout(invalidate, 0); });
document.addEventListener('focusin', (e) => { const h = hoverFrom(e.target); UI.hover = h; applyHover(); });
document.addEventListener('focusout', () => { UI.hover = null; applyHover(); });
document.addEventListener('mouseover', (e) => {
  const h = hoverFrom(e.target);
  if (JSON.stringify(h) !== JSON.stringify(UI.hover)) { UI.hover = h; applyHover(); }
});
document.addEventListener('click', (e) => {
  const r = e.target.closest('[data-range]'), f = e.target.closest('[data-filter]');
  if (r) { UI.range = r.dataset.range; return invalidate(); }
  if (f) { UI.filter = f.dataset.filter; return invalidate(); }
  if (e.target.closest('[data-follow]')) { UI.pinned = false; return invalidate(); }
  const h = hoverFrom(e.target);
  if (h?.token && !e.target.closest('a')) { UI.token = h.token; UI.pinned = true; invalidate(); }
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { UI.hover = null; applyHover(); return; }
  if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && e.target.closest('#rows')) {
    e.preventDefault();
    const btns = [...document.querySelectorAll('#rows .mk')];
    const i = btns.findIndex((b) => b.dataset.token === UI.token);
    const n = btns[Math.max(0, Math.min(btns.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))];
    if (n) { UI.token = n.dataset.token; UI.pinned = true; invalidate(); requestAnimationFrame(() => document.querySelector(`#rows [data-token="${CSS.escape(UI.token)}"]`)?.focus()); }
  }
});
new ResizeObserver(() => { last.clear(); invalidate(); }).observe(document.body);

// ---- données publiques (via serve.py, avec cache) ------------------------------------------------------------------
async function getJSON(url) { const r = await fetch(url); const j = await r.json(); if (!r.ok) throw new Error(j.error || r.status); return j; }
function wantMeta(tok) {
  if (!EXT.meta.has(tok) && Date.now() - (EXT.metaFail.get(tok) || 0) >= 20e3) EXT.metaWait.add(tok);
}
function flushMeta() {
  // les titres manquants partent groupés, une requête à la fois ; un échec est retenté 20 s plus tard
  if (EXT.metaBusy || !EXT.metaWait.size) return;
  const toks = [...EXT.metaWait].slice(0, 40);
  EXT.metaBusy = true;
  getJSON(`/api/markets?tokens=${toks.join(',')}`)
    .then((res) => { for (const t of toks) { if (res[t]?.title) EXT.meta.set(t, res[t]); else EXT.metaFail.set(t, Date.now()); } })
    .catch(() => toks.forEach((t) => EXT.metaFail.set(t, Date.now())))
    .finally(() => { toks.forEach((t) => EXT.metaWait.delete(t)); EXT.metaBusy = false; invalidate(); });
}
function wantHistory(tok, from) {
  const h = EXT.history.get(tok);
  const start = Math.floor(Math.min(from, M.run?.at ?? from) / 1000) - 3600;
  if (h && (h.loading || (Date.now() - h.at < 60e3 && h.start <= start))) return;
  EXT.history.set(tok, { ...(h || { points: [] }), loading: true });
  getJSON(`/api/history?token=${tok}&start=${start}`)
    .then((j) => { EXT.history.set(tok, { at: Date.now(), start, points: j.history || [] }); invalidate(); })
    .catch(() => EXT.history.set(tok, { at: Date.now(), start, points: h?.points || [] }));
}
function wantBook(tok) {
  if (EXT.bookToken !== tok) { EXT.bookToken = tok; EXT.book = null; loadBook(); }
}
function loadBook() {
  const tok = EXT.bookToken;
  if (!tok) return;
  getJSON(`/api/book?token=${tok}`).then((b) => { if (EXT.bookToken === tok) { EXT.book = b; invalidate(); } })
    .catch((e) => { if (EXT.bookToken === tok) { EXT.book = { error: String(e.message || e) }; invalidate(); } });
}
function loadPositions() {
  const w = M.run?.wallet;
  if (!w) return;
  getJSON(`/api/positions?wallet=${w}`).then((rows) => { EXT.positions = new Map(rows.map((r) => [String(r.asset), r])); invalidate(); }).catch(() => {});
}
setInterval(loadBook, 4000);
setInterval(loadPositions, 30000);
setInterval(invalidate, 1000); // horloge relative et segments « en file » qui s'allongent

// ---- flux du journal -----------------------------------------------------------------------------------------------
const es = new EventSource('/events');
es.addEventListener('reset', () => { M = createModel(); seen.clear(); UI.ready = false; last.clear(); invalidate(); });
es.addEventListener('batch', (e) => {
  for (const s of JSON.parse(e.data)) ingest(M, s);
  for (const x of M.trades) seen.add(x.tx);
  UI.ready = true; loadPositions(); invalidate();
});
es.addEventListener('line', (e) => {
  const hadRun = M.run?.wallet;
  try { ingest(M, JSON.parse(e.data)); } catch { return; }
  if (M.run?.wallet !== hadRun) loadPositions();
  invalidate();
});
es.addEventListener('status', (e) => { UI.conn = JSON.parse(e.data); invalidate(); });
es.onerror = () => { UI.conn = 'injoignable'; invalidate(); };
es.onopen = () => { if (UI.conn === 'injoignable') UI.conn = 'connexion'; };
invalidate();
