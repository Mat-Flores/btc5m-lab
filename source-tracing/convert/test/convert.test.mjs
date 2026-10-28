// Tests du convertisseur : `node --test source-tracing/convert/test/`
// Mini-sessions synthétiques : minuit traversé, deux hôtes le même jour, doublon, fins tronquées, lignes illisibles.
import { test, before } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CONVERT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'convert.mjs');
const T0 = Date.parse('2026-10-14T23:59:30Z'); // ms
const MID = Date.parse('2026-10-15T00:00:00Z');
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'conv-test-'));
const dirA = path.join(root, 'in1'), dirB = path.join(root, 'in2'), out = path.join(root, 'trace');

// gz tronqué comme un enregistrement interrompu : sync-flush sans fin de flux ni trailer
const truncGz = (text) => zlib.gzipSync(text, { finishFlush: zlib.constants.Z_SYNC_FLUSH });
const lines = (...o) => o.map((x) => (typeof x === 'string' ? x : JSON.stringify(x))).join('\n') + '\n';
const header = (id, host, nodeName) => ({ k: 'session', v: 1, id, config: { minEdge: 0.1 }, sources: ['binance'], host, ...(nodeName ? { nodeName } : {}), node: 'v24' });
const readJsonl = (f) => zlib.gunzipSync(fs.readFileSync(f)).toString().split('\n').filter(Boolean).map((l) => JSON.parse(l));
const sha = (f) => crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex');
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)]));
const rel = (f) => path.relative(out, f).split(path.sep).join('/');

let A; // lignes de la session A (pour comparer le `legacy`)
before(() => {
  fs.mkdirSync(dirA, { recursive: true }); fs.mkdirSync(dirB, { recursive: true });
  // Session A : laptop (hostname seul), 23:59:30 -> 00:01:50, traverse minuit, fin tronquée avec ligne partielle.
  A = [header('2026-10-14T23-59-30', 'mf-laptop')];
  for (let i = 0; i < 40; i++) A.push({ k: 'px', t: T0 + 1000 + i * 2000 + 0.123, src: 'binance', p: 81000 + i, ...(i % 2 ? { et: T0 + i * 2000 + 0.5 } : {}) });
  A.push({ k: 'px', t: 1792019507908.7163, src: 'coinbase', p: 81520.48, et: 1792019509233 }); // t fixe : 1792019507908716 µs (hors jours : voir plus bas)
  for (const dt of [1000, 2000, 3000, 40000, 110000]) A.push({ k: 'px', t: T0 + dt, src: 'okx', p: 81500, et: T0 + dt - 100 }); // silence de 70 s le jour 2
  for (const dt of [1000, 2000, 3000, 15000, 16000]) A.push({ k: 'cl', t: T0 + dt, p: 81000.5, ts: T0 + dt - 200 }); // silence de 12 s (> 10 s)
  A.push({ k: 'meta', t: T0 + 500, w: { slug: 'btc-updown-5m-1', start: 1, len: 300 } });
  A.push({ k: 'book', t: T0 + 1500, a: 'UP', bids: [[0.4, 10]], asks: [[0.6, 10]], ts: T0 + 1400 });
  A.push({ k: 'book', t: T0 + 1600, a: 'UP', bids: [[0.4, 10]], asks: [[0.6, 10]], ts: 0 });
  A.push({ k: 'book', t: T0 + 1700, a: 'DN', bids: [[0.4, 10]], asks: [[0.6, 10]] });
  A.push({ k: 'delta', t: T0 + 1800, ch: [['UP', 'B', 0.4, 5]], ts: T0 + 1790 });
  A.push({ k: 'delta', t: T0 + 40500, ch: [['UP', 'B', 0.4, 6]], ts: T0 + 40490 });
  A.push({ k: 'trade', t: T0 + 41000, a: 'UP', p: 0.5, s: 3, side: 'BUY', id: 'x', ts: T0 + 40990 });
  A.push({ k: 'rtt', t: T0 + 2500, ms: 25 }, { k: 'rtt', t: T0 + 41500, ms: 26 });
  A.push({ k: 'ptb', t: T0 + 45000, slug: 'btc-updown-5m-1', priceToBeat: 81000 });
  A.push({ k: 'res', t: T0 + 46000, slug: 'btc-updown-5m-1', winner: 'Up' });
  A.push({ k: 'clock', t: T0 + 100, offset: -3.5 }, { k: 'clock', t: T0 + 35000, offset: 4 }, { k: 'clock', t: T0 + 100000, offset: 1.25 });
  A.push({ k: 'cfg', t: T0 + 3000, patch: { paused: true } }, { k: 'lane', t: T0 + 3100 }, { k: 'real', t: T0 + 3200, type: 'armed' });
  A.push({ k: 'zzz', t: T0 + 3300 }, { k: 'px', t: T0 + 3400, src: 'fastest', p: 1 });
  const textA = lines(...A.slice(0, 20), 'pas du json {', ...A.slice(20)) + '{"k":"px","t":1792';
  fs.writeFileSync(path.join(dirA, '2026-10-14T23-59-30.jsonl.gz'), truncGz(textA));
  // Doublon de A (copie plus courte, ailleurs) : doit être écarté
  fs.writeFileSync(path.join(dirB, '2026-10-14T23-59-30.jsonl.gz'), truncGz(lines(...A.slice(0, 10))));
  // Session B : dublin (nodeName), 00:05:00, beaucoup plus de lignes binance le 15 -> hôte canonique ce jour-là
  const B = [header('2026-10-15T00-05-00_dublin', 'ip-172-31-19-166', 'dublin')];
  for (let i = 0; i < 100; i++) B.push({ k: 'px', t: MID + 300000 + i * 1000, src: 'binance', p: 80000 + i });
  fs.writeFileSync(path.join(dirB, '2026-10-15T00-05-00_dublin.jsonl.gz'), zlib.gzipSync(lines(...B)));
  // Session A2 : même hôte que A, 00:10:00 -> trou « no_recording » entre les deux sessions
  const A2 = [header('2026-10-15T00-10-00', 'mf-laptop')];
  for (let i = 0; i < 5; i++) A2.push({ k: 'px', t: MID + 600000 + 500 + i * 1000, src: 'binance', p: 79000 + i });
  A2.push({ k: 'clock', t: MID + 600100, offset: 2 });
  fs.writeFileSync(path.join(dirA, '2026-10-15T00-10-00.jsonl.gz'), zlib.gzipSync(lines(...A2)));
  // Exclues : synthétique et vide
  fs.writeFileSync(path.join(dirA, '2026-10-16T09-00-00_sim.jsonl.gz'), zlib.gzipSync(lines({ k: 'session', v: 1, id: '2026-10-16T09-00-00_sim' }, { k: 'px', t: T0, src: 'binance', p: 1 })));
  fs.writeFileSync(path.join(dirA, '2026-10-15T20-37-05.jsonl.gz'), '');
  // Fichier non-session ignoré par le listage (pas .jsonl.gz)
  fs.writeFileSync(path.join(dirA, '2026-10-14T23-59-30.records.jsonl'), '{}\n');
});

const convert = (...args) => execFileSync(process.execPath, [CONVERT, '--out', out, ...args, dirA, dirB], { encoding: 'utf8', maxBuffer: 1 << 26 });

test('conversion : fichiers, découpage jour/hôte, D5, exclusions, doublon', () => {
  convert();
  const files = walk(out).map(rel).sort();
  const expect = (s, d, name) => `${s}/2026/10/${d}${name}`;
  assert.ok(files.includes(expect('binance-btcusdt', '2026-10-14', '.old.jsonl.gz')));
  assert.ok(files.includes(expect('binance-btcusdt', '2026-10-15', '.old.jsonl.gz')), 'dublin canonique le 15');
  assert.ok(files.includes(expect('binance-btcusdt', '2026-10-15', '.local.old.jsonl.gz')), 'local = hôte secondaire (D5)');
  assert.ok(files.includes(expect('binance-btcusdt', '2026-10-15', '.local.old.manifest.json')));
  for (const s of ['okx-btc-usdt-swap', 'chainlink-rtds', 'polymarket-clob', 'polymarket-gamma', 'polymarket-clob-time', 'coinbase-btc-usd']) assert.ok(files.some((f) => f.startsWith(`${s}/`)), s);
  assert.ok(!files.some((f) => f.endsWith('.tmp')), 'aucun .tmp restant');
  assert.ok(!files.some((f) => /2026-10-16|2026-10-15T20/.test(f)), 'sessions exclues');
  const report = JSON.parse(fs.readFileSync(path.join(root, 'convert-report.json'), 'utf8'));
  assert.equal(report.duplicates.length, 1);
  assert.equal(report.sessions.find((s) => s.id === '2026-10-16T09-00-00_sim').status, 'exclue');
  assert.equal(report.sessions.find((s) => s.id === '2026-10-14T23-59-30').truncated_input, true);
  assert.equal(report.sessions.find((s) => s.id === '2026-10-14T23-59-30').dropped_partial_tail, true);
  assert.equal(report.sessions.find((s) => s.id === '2026-10-14T23-59-30').unreadable.count, 1);
  assert.equal(report.sessions.find((s) => s.id === '2026-10-14T23-59-30').not_converted.cfg, 1);
  assert.deepEqual(report.sessions.find((s) => s.id === '2026-10-14T23-59-30').ignored, { 'inconnu:zzz': 1, 'px:fastest': 1 });
  assert.equal(report.totals.conservation_ok, true);
});

test('lignes : enveloppe, t_recv en µs, t_src, conn, legacy conservé, raw null', () => {
  const d1 = readJsonl(path.join(out, 'binance-btcusdt/2026/10/2026-10-14.old.jsonl.gz'));
  const d2 = readJsonl(path.join(out, 'binance-btcusdt/2026/10/2026-10-15.local.old.jsonl.gz'));
  assert.deepEqual(Object.keys(d1[0]), ['seq', 't_recv', 't_src', 'seq_src', 'conn', 'kind', 'raw', 'legacy', 'inferred']);
  assert.equal(d1[0].kind, 'recorder_start');
  assert.equal(d1[0].t_recv, Date.parse('2026-10-14T23:59:30Z') * 1000);
  assert.equal(d1[0].inferred, 'id');
  assert.equal(d1[0].legacy.id, '2026-10-14T23-59-30');
  const mids = d1.filter((l) => l.kind === 'mid');
  const origMids = A.filter((o) => o.k === 'px' && o.src === 'binance' && o.t < MID);
  assert.equal(mids.length, origMids.length);
  mids.forEach((l, i) => {
    const o = origMids[i];
    assert.equal(l.t_recv, Math.round(o.t * 1000));
    assert.equal(l.t_src, o.et === undefined ? null : Math.round(o.et * 1000));
    assert.deepEqual(l.legacy, o);
    assert.equal(l.raw, null); assert.equal(l.seq_src, null); assert.equal(l.conn, 0);
    assert.ok(Number.isInteger(l.t_recv) && l.t_recv < MID * 1000);
  });
  // continuation après minuit : recorder_start « rotate » à 00:00:00 puis lignes du jour 2 (session A), puis trou, puis A2
  assert.equal(d2[0].kind, 'recorder_start'); assert.equal(d2[0].t_recv, MID * 1000); assert.equal(d2[0].inferred, 'rotate');
  const gap = d2.find((l) => l.kind === 'gap');
  assert.equal(gap.reason, 'no_recording'); assert.equal(gap.inferred, true); assert.equal(gap.seq, null);
  assert.equal(gap.to, Date.parse('2026-10-15T00:10:00Z') * 1000);
  assert.ok(gap.from < gap.to);
  assert.equal(d2.filter((l) => l.kind === 'recorder_start').length, 2);
  // t fixe du contrat
  const cb = readJsonl(path.join(out, 'coinbase-btc-usd/2026/10', fs.readdirSync(path.join(out, 'coinbase-btc-usd/2026/10')).find((f) => f.endsWith('.old.jsonl.gz'))));
  assert.equal(cb.find((l) => l.kind === 'mid').t_recv, 1792019507908716);
  assert.equal(cb.find((l) => l.kind === 'mid').t_src, 1792019509233000);
});

test('kinds : snapshot/delta/trade, derived, ts==0, window/resolution/price_to_beat/rtt, oracle, clock', () => {
  const clob = [...readJsonl(path.join(out, 'polymarket-clob/2026/10/2026-10-14.old.jsonl.gz')), ...readJsonl(path.join(out, 'polymarket-clob/2026/10/2026-10-15.old.jsonl.gz'))];
  const snaps = clob.filter((l) => l.kind === 'snapshot');
  assert.equal(snaps.length, 3);
  assert.equal(snaps[0].t_src, (T0 + 1400) * 1000);
  assert.equal(snaps[1].t_src, null); assert.equal(snaps[1].derived, undefined); // ts == 0
  assert.equal(snaps[2].t_src, null); assert.equal(snaps[2].derived, true);       // pas de ts
  assert.equal(clob.filter((l) => l.kind === 'delta').length, 2);
  const tr = clob.find((l) => l.kind === 'trade');
  assert.equal(tr.conn, null); assert.equal(tr.t_src, (T0 + 40990) * 1000);
  const gam = readJsonl(path.join(out, 'polymarket-gamma/2026/10/2026-10-14.old.jsonl.gz')).concat(readJsonl(path.join(out, 'polymarket-gamma/2026/10/2026-10-15.old.jsonl.gz')));
  assert.deepEqual(gam.filter((l) => !['recorder_start', 'clock', 'gap'].includes(l.kind)).map((l) => l.kind).sort(), ['price_to_beat', 'resolution', 'window']);
  assert.equal(readJsonl(path.join(out, 'polymarket-clob-time/2026/10/2026-10-14.old.jsonl.gz')).filter((l) => l.kind === 'rtt').length, 1);
  const orc = readJsonl(path.join(out, 'chainlink-rtds/2026/10/2026-10-14.old.jsonl.gz'));
  assert.equal(orc.filter((l) => l.kind === 'oracle').length, 5);
  assert.equal(orc.find((l) => l.kind === 'oracle').t_src, (T0 + 800) * 1000);
  // clock : 3 horloges (1 le jour 1, 2 le jour 2) copiées dans chaque fichier source du jour ; offset_ms + legacy ; t_recv non corrigé
  const ck = readJsonl(path.join(out, 'chainlink-rtds/2026/10/2026-10-14.old.jsonl.gz')).filter((l) => l.kind === 'clock');
  assert.equal(ck.length, 1); assert.equal(ck[0].offset_ms, -3.5); assert.equal(ck[0].t_src, null); assert.deepEqual(ck[0].legacy, { k: 'clock', t: T0 + 100, offset: -3.5 });
  assert.equal(ck[0].t_recv, Math.round((T0 + 100) * 1000));
});

test('manifestes : sha256 recalculé, lignes, heures, trous (silence, entre sessions, bords)', () => {
  const all = walk(out).filter((f) => f.endsWith('.manifest.json'));
  assert.ok(all.length >= 10);
  for (const mf of all) {
    const m = JSON.parse(fs.readFileSync(mf, 'utf8'));
    const data = path.join(path.dirname(mf), m.file);
    assert.equal(m.sha256, sha(data), mf); assert.equal(m.bytes, fs.statSync(data).size);
    const ls = readJsonl(data);
    assert.equal(m.lines, ls.length);
    assert.equal(Object.values(m.lines_by_kind).reduce((a, b) => a + b, 0), ls.length);
    assert.equal(m.hours.length, 24); assert.equal(m.hours.reduce((n, h) => n + h.lines, 0), ls.length);
    assert.equal(m.first_t_recv, Math.min(...ls.map((l) => l.t_recv))); assert.equal(m.last_t_recv, Math.max(...ls.map((l) => l.t_recv)));
    assert.equal(m.format, 'old'); assert.equal(m.schema_version, 1);
    assert.equal(m.runs.reduce((n, r) => n + r.lines, 0), ls.length);
    assert.ok(m.gaps.every((g) => g.origin === 'inferred' && g.to > g.from));
    assert.ok(m.coverage >= 0 && m.coverage <= 1);
    assert.ok(Math.abs(m.covered_s + m.gaps_total_s - 86400) < 1e-6);
  }
  const okx2 = JSON.parse(fs.readFileSync(path.join(out, 'okx-btc-usdt-swap/2026/10/2026-10-15.old.manifest.json'), 'utf8'));
  assert.equal(okx2.silence_threshold_ms, 60000);
  assert.ok(okx2.gaps.some((g) => g.reason === 'silence' && g.to - g.from === 70_000_000));
  const cl1 = JSON.parse(fs.readFileSync(path.join(out, 'chainlink-rtds/2026/10/2026-10-14.old.manifest.json'), 'utf8'));
  assert.ok(cl1.gaps.some((g) => g.reason === 'silence' && g.to - g.from === 12_000_000));
  assert.deepEqual(cl1.legacy, { sessions: ['2026-10-14T23-59-30'], truncated_input: true });
  assert.equal(cl1.runs[0].clean_end, false);
  const b2 = JSON.parse(fs.readFileSync(path.join(out, 'binance-btcusdt/2026/10/2026-10-15.local.old.manifest.json'), 'utf8'));
  assert.equal(b2.host_id, 'local'); assert.equal(b2.runs.length, 2);
  assert.ok(b2.gaps.some((g) => g.reason === 'no_recording' && g.to === Date.parse('2026-10-15T00:10:00Z') * 1000));
  const g1 = JSON.parse(fs.readFileSync(path.join(out, 'polymarket-gamma/2026/10/2026-10-14.old.manifest.json'), 'utf8'));
  assert.equal(g1.silence_threshold_ms, null);
});

test('idempotence : 2e exécution = mêmes octets (données et manifestes)', () => {
  const snap = () => Object.fromEntries(walk(out).filter((f) => !f.endsWith('convert-report.json')).map((f) => [rel(f), sha(f)]));
  const before1 = snap();
  convert();
  assert.deepEqual(snap(), before1);
});

test('--dry-run : plan sans rien écrire', () => {
  const o2 = path.join(root, 'trace-dry');
  const txt = execFileSync(process.execPath, [CONVERT, '--out', o2, '--dry-run', dirA, dirB], { encoding: 'utf8' });
  assert.ok(!fs.existsSync(o2), 'rien écrit');
  assert.match(txt, /PLAN \(rien écrit\)/);
  assert.match(txt, /binance-btcusdt\/2026\/10\/2026-10-15\.local\.old\.jsonl\.gz/);
  const real = JSON.parse(fs.readFileSync(path.join(out, 'binance-btcusdt/2026/10/2026-10-15.local.old.manifest.json'), 'utf8'));
  assert.match(txt, new RegExp(`2026-10-15\\.local\\.old\\.jsonl\\.gz\\s+${real.lines} lignes`));
});
