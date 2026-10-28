#!/usr/bin/env node
// Convertisseur des anciennes sessions btc5m-lag-sim (`{"k":…,"t":…}`) vers le format de trace v1 (`.old`).
// Contrat : CONTRATS.md "Contrat 1" ; mapping détaillé : MAPPING.md. Aucune dépendance (node:* seulement).
// Lecture en flux (jamais une session entière en mémoire) ; écriture `.tmp` puis renommage (données, puis manifeste).
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import { once } from 'node:events';
import { StringDecoder } from 'node:string_decoder';
import { pathToFileURL } from 'node:url';

export const VERSION = 'convert-1';
const DAY = 86_400_000_000; // µs
const HOUR = 3_600_000_000;
const MAX_UNREADABLE_SAMPLES = 10;

// hostname -> nom de nœud (table du contrat) ; `nodeName` de l'en-tête prime.
const HOSTS = { 'ip-172-31-19-166': 'dublin', 'ip-172-31-28-37': 'dry-run', 'ip-172-31-28-36': 'live', 'mf-laptop': 'local' };
// ancien `px.src` -> source
const PX_SOURCE = {
  binance: 'binance-btcusdt', binanceFut: 'binance-perp-btcusdt', coinbase: 'coinbase-btc-usd', bybit: 'bybit-btcusdt-linear',
  okx: 'okx-btc-usdt-swap', kraken: 'kraken-btc-usd', bitstamp: 'bitstamp-btcusd',
};
// ancien k -> {source, kind} (px et clock traités à part)
const MAP = {
  cl: ['chainlink-rtds', 'oracle'], book: ['polymarket-clob', 'snapshot'], delta: ['polymarket-clob', 'delta'], trade: ['polymarket-clob', 'trade'],
  meta: ['polymarket-gamma', 'window'], res: ['polymarket-gamma', 'resolution'], ptb: ['polymarket-gamma', 'price_to_beat'], rtt: ['polymarket-clob-time', 'rtt'],
};
const NOT_CONVERTED = new Set(['cfg', 'lane', 'real']);
// Seuil de silence (ms) et kinds concernés par source. mid : 60 s (contrat) ; oracle Chainlink 10 s ; CLOB Polymarket 3 s [à vérifier] ;
// HTTP (gamma, clob-time) : pas de seuil.
const SILENCE = {};
for (const s of Object.values(PX_SOURCE)) SILENCE[s] = { kinds: new Set(['mid']), ms: 60_000 };
SILENCE['chainlink-rtds'] = { kinds: new Set(['oracle']), ms: 10_000 };
SILENCE['polymarket-clob'] = { kinds: new Set(['snapshot', 'delta', 'trade']), ms: 3_000 };
// Sessions exclues par le contrat (en plus du « vide » détecté automatiquement).
export const EXCLUDED = { '2026-10-16T09-00-00_sim': 'synthétique (contrat)', '2026-10-15T20-37-05': 'vide (contrat)' };

// ----------------------------------------------------------------------------------------------- utilitaires
const dayIdx = (us) => Math.floor(us / DAY);
const dayStr = (d) => new Date(d * 86_400_000).toISOString().slice(0, 10);
const pad = (n) => String(n).padStart(2, '0');
const us = (ms) => (typeof ms === 'number' && Number.isFinite(ms) && ms > 0 ? Math.round(ms * 1000) : null); // 0/absent = pas d'horodatage
const safe = (s) => String(s).replace(/[^A-Za-z0-9_-]/g, '_');
const sidToUs = (id) => { // "2026-10-14T20-43-50[_hôte]" -> µs, à la seconde
  const m = /^(\d{4})-(\d\d)-(\d\d)T(\d\d)-(\d\d)-(\d\d)/.exec(id);
  return m ? Date.parse(`${m[1]}-${m[2]}-${m[3]}T${m[4]}:${m[5]}:${m[6]}Z`) * 1000 : null;
};
const secs = (x) => Number((x / 1e6).toFixed(6));

/** Lit en flux les lignes d'un .jsonl.gz. Rend {truncated, error, droppedTail}.
 *  Fin tronquée tolérée : finishFlush SYNC_FLUSH ne lève aucune erreur et n'abandonne aucune donnée déjà décompressée (avec Z_FINISH,
 *  l'itérateur jette le tampon à l'erreur, de façon non déterministe). La troncature est détectée en comparant le trailer gzip du fichier
 *  (CRC32 + taille mod 2^32, 8 derniers octets) à ceux du flux décompressé. Limite : fichier multi-membres -> signalé tronqué à tort. */
async function readLines(file, onLine) {
  const gz = zlib.createGunzip({ chunkSize: 1 << 20, finishFlush: zlib.constants.Z_SYNC_FLUSH });
  const rs = fs.createReadStream(file, { highWaterMark: 1 << 20 });
  rs.on('error', (e) => gz.destroy(e));
  rs.pipe(gz);
  const dec = new StringDecoder('utf8');
  let rest = '', truncated = false, error = null, droppedTail = false, stop = false, crc = 0, total = 0;
  try {
    for await (const chunk of gz) {
      crc = zlib.crc32(chunk, crc); total += chunk.length;
      const s = rest + dec.write(chunk);
      let from = 0, i;
      while ((i = s.indexOf('\n', from)) >= 0) {
        const p = onLine(s.slice(from, i));
        if (p) { if ((await p) === false) { stop = true; break; } } else if (p === false) { stop = true; break; }
        from = i + 1;
      }
      if (stop) { rest = ''; break; }
      rest = s.slice(from);
    }
  } catch (e) { truncated = true; error = e.code || e.message; }
  if (stop) { gz.destroy(); rs.destroy(); }
  else if (!error) {
    const fd = fs.openSync(file, 'r'); const t = Buffer.alloc(8);
    const size = fs.fstatSync(fd).size;
    if (size >= 8) fs.readSync(fd, t, 0, 8, size - 8);
    fs.closeSync(fd);
    truncated = size < 18 || t.readUInt32LE(0) !== crc || t.readUInt32LE(4) !== total % 2 ** 32;
  }
  rest += dec.end();
  if (rest.length && !stop) {
    if (truncated) droppedTail = true; // fin tronquée : on s'arrête à la dernière ligne complète
    else await onLine(rest);
  }
  return { truncated, error, droppedTail };
}

async function readHeader(file) { // 1re ligne (texte) ou null
  let first = null;
  const size = fs.statSync(file).size;
  if (size === 0) return { size, text: null };
  await readLines(file, (l) => { first = l; return false; });
  return { size, text: first };
}

async function sha256File(file) {
  const h = crypto.createHash('sha256');
  for await (const c of fs.createReadStream(file, { highWaterMark: 1 << 20 })) h.update(c);
  return h.digest('hex');
}

// ----------------------------------------------------------------------------------------------- sortie
class OutFile {
  constructor(source, d, host, dir, dry) {
    this.source = source; this.d = d; this.host = host; this.dir = dir; this.dry = dry;
    this.tmp = path.join(dir, `${dayStr(d)}.${host}.old.jsonl.gz.tmp`);
    this.lines = 0; this.byKind = {}; this.hours = new Array(24).fill(0);
    this.first = null; this.last = null; this.runs = []; this.gaps = []; this.sessions = []; this.truncated = false;
    this.prevEnd = null; this.buf = ''; this.bytes = 0;
    if (!dry) {
      fs.mkdirSync(dir, { recursive: true });
      this.fd = fs.openSync(this.tmp, 'w');
      this.hash = crypto.createHash('sha256');
    }
  }
  openMember() {
    if (this.dry) return;
    this.gz = zlib.createGzip({ level: 6, chunkSize: 1 << 18, writableHighWaterMark: 8 << 20 });
    this.gz.on('data', (c) => { this.hash.update(c); fs.writeSync(this.fd, c); this.bytes += c.length; });
    this.gz.on('error', (e) => { this.err = e; });
  }
  async flush() {
    if (this.dry || !this.buf) return;
    if (this.err) throw this.err;
    const b = Buffer.from(this.buf); this.buf = '';
    if (!this.gz.write(b)) await once(this.gz, 'drain');
  }
  async closeMember() {
    if (this.dry) return;
    await this.flush();
    this.gz.end();
    await once(this.gz, 'end');
    if (this.err) throw this.err;
    this.gz = null;
  }
  /** Ajoute une ligne ; rend une promesse seulement si un vidage du tampon est nécessaire. */
  add(seg, kind, tr, text) {
    this.lines++; seg.lines++;
    this.byKind[kind] = (this.byKind[kind] || 0) + 1;
    this.hours[Math.floor((tr - this.d * DAY) / HOUR)]++;
    if (this.first === null || tr < this.first) this.first = tr;
    if (this.last === null || tr > this.last) this.last = tr;
    if (seg.first === null || tr < seg.first) seg.first = tr;
    if (seg.last === null || tr > seg.last) seg.last = tr;
    if (this.dry) return null;
    this.buf += text + '\n';
    return this.buf.length > (1 << 18) ? this.flush() : null;
  }
  async close() {
    if (this.dry) return;
    fs.fsyncSync(this.fd); fs.closeSync(this.fd);
  }
}

const env = (seq, tr, tsrc, conn, kind) => `{"seq":${seq},"t_recv":${tr},"t_src":${tsrc},"seq_src":null,"conn":${conn},"kind":"${kind}","raw":null`;

// ----------------------------------------------------------------------------------------------- plan (entrées, dédoublonnage)
export function listInputs(args) {
  const files = [];
  for (const a of args) {
    const st = fs.statSync(a);
    if (st.isDirectory()) for (const f of fs.readdirSync(a).sort()) { if (f.endsWith('.jsonl.gz') && fs.statSync(path.join(a, f)).isFile()) files.push(path.join(a, f)); }
    else files.push(a);
  }
  return [...new Set(files.map((f) => path.resolve(f)))];
}

async function plan(files, report) {
  const byId = new Map();
  for (const file of files) {
    const { size, text } = await readHeader(file);
    const base = path.basename(file).replace(/\.jsonl\.gz$/, '');
    let h = null;
    if (text) { try { h = JSON.parse(text); } catch { /* en-tête illisible */ } }
    if (!h || h.k !== 'session') {
      report.sessions.push({ id: base, file, bytes: size, status: size === 0 ? 'exclue' : 'ignorée', reason: size === 0 ? 'vide (fichier de 0 octet)' : 'première ligne illisible ou pas un en-tête session' });
      continue;
    }
    const id = String(h.id ?? base);
    (byId.get(id) ?? byId.set(id, []).get(id)).push({ file, size, text, header: h, id });
  }
  const kept = [];
  for (const [id, cands] of byId) {
    cands.sort((a, b) => b.size - a.size || (a.file < b.file ? -1 : 1)); // le plus gros = le plus complet
    const [best, ...dups] = cands;
    best.duplicates = [];
    for (const d of dups) {
      let same = null;
      if (d.size === best.size) same = (await sha256File(d.file)) === (await sha256File(best.file));
      best.duplicates.push({ file: d.file, bytes: d.size, identique: same, note: same === null ? 'taille différente : le plus gros est conservé' : same ? 'octets identiques' : 'même taille, contenu différent' });
      report.duplicates.push({ id, kept: best.file, dropped: d.file, bytes_kept: best.size, bytes_dropped: d.size, identical: same });
    }
    if (EXCLUDED[id]) { report.sessions.push({ id, file: best.file, bytes: best.size, status: 'exclue', reason: EXCLUDED[id], duplicates: best.duplicates }); continue; }
    best.nodeName = best.header.nodeName || null;
    best.hostname = best.header.host || null;
    best.host = safe(best.nodeName || HOSTS[best.hostname] || best.hostname || 'unknown');
    if (!best.nodeName && !HOSTS[best.hostname]) report.warnings.push(`session ${id} : hôte inconnu (${best.hostname}), nom utilisé : ${best.host}`);
    best.startUs = sidToUs(id);
    if (best.startUs === null) { report.sessions.push({ id, file: best.file, bytes: best.size, status: 'ignorée', reason: 'id de session sans date lisible' }); continue; }
    best.engine = best.header.engine != null; // capture par l'engine Rust (en-tête `engine`) -> conn null
    kept.push(best);
  }
  kept.sort((a, b) => a.startUs - b.startUs || (a.id < b.id ? -1 : 1));
  return kept;
}

// ----------------------------------------------------------------------------------------------- conversion d'une session
async function convertSession(S, G) {
  const R = {
    id: S.id, file: S.file, bytes: S.size, host: S.host, hostname: S.hostname, status: 'convertie', lines_read: 0,
    read_by_kind: {}, converted_by_kind: {}, written_by_kind: {}, not_converted: {}, ignored: {}, unreadable: { count: 0, lines: [] },
    clock: { read: 0, copies_written: 0 }, lifecycle_written: { recorder_start: 0, gap: 0 }, truncated_input: false, dropped_partial_tail: false, files: [],
    duplicates: S.duplicates,
  };
  const segs = new Map(), last = new Map(); // `${source}|${d}` -> seg ; source -> dernier seg
  const clocks = [];
  let lineNo = -1;

  const getSeg = async (source, tr) => {
    const d = dayIdx(tr);
    const l = last.get(source);
    if (l && l.d === d) return l;
    const key = `${source}|${d}`;
    let seg = segs.get(key);
    if (!seg) {
      const fk = `${source}|${d}|${S.host}`;
      let f = G.files.get(fk);
      if (!f) {
        f = new OutFile(source, d, S.host, path.join(G.out, source, dayStr(d).slice(0, 4), dayStr(d).slice(5, 7)), G.dry);
        G.files.set(fk, f);
      }
      seg = { f, d, source, lines: 0, first: null, last: null, ci: 0, silPrev: null, sil: SILENCE[source] };
      segs.set(key, seg);
      f.sessions.push(S.id);
      f.openMember();
      const dayStart = d * DAY;
      const fromId = dayIdx(S.startUs) === d;
      const rs = fromId ? S.startUs : dayStart;
      if (f.prevEnd !== null && rs > f.prevEnd) { // 2e session (ou suivante) du même hôte/jour : trou déduit
        f.gaps.push({ from: f.prevEnd, to: rs, reason: 'no_recording', origin: 'inferred' });
        await f.add(seg, 'gap', rs, `${env('null', rs, 'null', 'null', 'gap')},"reason":"no_recording","from":${f.prevEnd},"to":${rs},"inferred":true}`);
        R.lifecycle_written.gap++;
      }
      await f.add(seg, 'recorder_start', rs, `${env(0, rs, 'null', S.engine ? 'null' : 0, 'recorder_start')},"legacy":${S.text},"inferred":${fromId ? '"id"' : '"rotate"'}}`);
      R.lifecycle_written.recorder_start++;
    }
    last.set(source, seg);
    return seg;
  };
  const flushClocks = async (seg, upTo) => { // horloges du jour déjà reçues (t_recv <= upTo), dans l'ordre
    while (seg.ci < clocks.length && (upTo === null || clocks[seg.ci].tr <= upTo)) {
      const c = clocks[seg.ci++];
      if (c.d !== seg.d) continue;
      await seg.f.add(seg, 'clock', c.tr, c.line);
      R.clock.copies_written++;
    }
  };
  const count = (o, k) => { o[k] = (o[k] || 0) + 1; };
  const bad = (n) => { R.unreadable.count++; if (R.unreadable.lines.length < MAX_UNREADABLE_SAMPLES) R.unreadable.lines.push(n); };

  const onLine = (raw) => {
    lineNo++;
    if (lineNo === 0) return; // en-tête, déjà lu au plan
    const text = raw.endsWith('\r') ? raw.slice(0, -1) : raw;
    if (text === '') return;
    R.lines_read++;
    let o;
    try { o = JSON.parse(text); } catch { return bad(lineNo); }
    if (o === null || typeof o !== 'object' || typeof o.k !== 'string' || typeof o.t !== 'number' || !Number.isFinite(o.t)) return bad(lineNo);
    const k = o.k;
    count(R.read_by_kind, k);
    const tr = Math.round(o.t * 1000);
    if (k === 'clock') {
      R.clock.read++;
      const conn = S.engine ? 'null' : 0;
      const off = typeof o.offset === 'number' && Number.isFinite(o.offset) ? o.offset : null;
      clocks.push({ tr, d: dayIdx(tr), line: `${env(lineNo, tr, 'null', conn, 'clock')},"offset_ms":${off},"legacy":${text}}` });
      return;
    }
    if (NOT_CONVERTED.has(k)) return count(R.not_converted, k);
    let source, kind, tsrc = null, extra = '';
    if (k === 'px') {
      source = PX_SOURCE[o.src];
      if (!source) return count(R.ignored, `px:${o.src}`);
      kind = 'mid'; tsrc = us(o.et);
    } else if (MAP[k]) {
      [source, kind] = MAP[k];
      if (k === 'cl' || k === 'book' || k === 'delta' || k === 'trade') tsrc = us(o.ts);
      if ((k === 'book' || k === 'delta') && (o.ts === undefined || o.ts === null)) extra = ',"derived":true';
    } else if (k === 'session') return count(R.ignored, 'session(en-tête répété)');
    else return count(R.ignored, `inconnu:${k}`);
    count(R.converted_by_kind, k);
    const conn = S.engine || k === 'trade' ? 'null' : 0;
    return (async () => {
      const seg = await getSeg(source, tr);
      await flushClocks(seg, tr);
      if (seg.sil && seg.sil.kinds.has(kind)) { // silence : écart entre deux lignes de marché de la même session
        if (seg.silPrev !== null && tr - seg.silPrev > seg.sil.ms * 1000) seg.f.gaps.push({ from: seg.silPrev, to: tr, reason: 'silence', origin: 'inferred' });
        if (seg.silPrev === null || tr > seg.silPrev) seg.silPrev = tr;
      }
      await seg.f.add(seg, kind, tr, `${env(lineNo, tr, tsrc === null ? 'null' : tsrc, conn, kind)},"legacy":${text}${extra}}`);
      count(R.written_by_kind, kind);
    })();
  };

  const prog = setInterval(() => process.stderr.write(`  … ${S.id} : ${R.lines_read.toLocaleString('fr-FR')} lignes\n`), 60_000);
  let res;
  try { res = await readLines(S.file, onLine); } finally { clearInterval(prog); }
  R.truncated_input = res.truncated; R.dropped_partial_tail = res.droppedTail;
  if (res.error) R.read_error = res.error;
  // fin de session : horloges restantes, fermeture des membres gzip, enregistrement du lancement
  for (const seg of segs.values()) {
    await flushClocks(seg, null);
    await seg.f.closeMember();
    seg.f.prevEnd = seg.f.prevEnd === null ? seg.last : Math.max(seg.f.prevEnd, seg.last);
    if (res.truncated) seg.f.truncated = true;
    seg.f.runs.push({ run: S.id, host_id: S.host, recorder_version: null, first_t_recv: seg.first, last_t_recv: seg.last, lines: seg.lines, clean_end: !res.truncated });
    R.files.push({ source: seg.source, date: dayStr(seg.d), lines: seg.lines });
  }
  const hasClockOnly = clocks.length > 0 && segs.size === 0;
  if (hasClockOnly) R.warnings = ['lignes clock sans fichier de source : non écrites'];
  const inMapped = Object.entries(R.converted_by_kind).reduce((n, [, v]) => n + v, 0);
  const outData = Object.values(R.written_by_kind).reduce((n, v) => n + v, 0);
  R.check = { mapped_in: inMapped, data_out: outData, ok: inMapped === outData };
  return R;
}

// ----------------------------------------------------------------------------------------------- manifestes
function buildManifest(f, finalName, validatedAt) {
  const dayStart = f.d * DAY, dayEnd = dayStart + DAY;
  const gaps = [...f.gaps];
  if (f.first > dayStart) gaps.push({ from: dayStart, to: f.first, reason: 'no_recording', origin: 'inferred' });
  if (f.last < dayEnd) gaps.push({ from: f.last, to: dayEnd, reason: 'no_recording', origin: 'inferred' });
  gaps.sort((a, b) => a.from - b.from || a.to - b.to);
  const merged = []; // union des trous, pour la couverture
  for (const g of gaps) { const l = merged[merged.length - 1]; if (l && g.from <= l[1]) l[1] = Math.max(l[1], g.to); else merged.push([g.from, g.to]); }
  const overlap = (a, b) => merged.reduce((n, [x, y]) => n + Math.max(0, Math.min(y, b) - Math.max(x, a)), 0);
  const total = merged.reduce((n, [x, y]) => n + (y - x), 0);
  const sil = SILENCE[f.source];
  return {
    schema_version: 1, source: f.source, date: dayStr(f.d), file: finalName, format: 'old', host_id: f.host, bytes: f.bytes, sha256: f.hash.digest('hex'),
    lines: f.lines, lines_by_kind: Object.fromEntries(Object.entries(f.byKind).sort(([a], [b]) => (a < b ? -1 : 1))),
    first_t_recv: f.first, last_t_recv: f.last, runs: f.runs,
    hours: f.hours.map((n, h) => ({ h, lines: n, covered_s: secs(HOUR - overlap(dayStart + h * HOUR, dayStart + (h + 1) * HOUR)) })),
    covered_s: secs(DAY - total), coverage: Number(((DAY - total) / DAY).toFixed(6)),
    gaps, gaps_total_s: secs(total), disconnects: 0, decode_errors: 0, silence_threshold_ms: sil ? sil.ms : null,
    legacy: { sessions: [...new Set(f.sessions)], truncated_input: f.truncated }, validated_at: validatedAt, validator_version: VERSION,
  };
}

async function finalize(G, report) {
  const groups = new Map();
  for (const f of G.files.values()) { const k = `${f.source}|${f.d}`; (groups.get(k) ?? groups.set(k, []).get(k)).push(f); }
  for (const arr of groups.values()) {
    arr.sort((a, b) => b.lines - a.lines || (a.host < b.host ? -1 : 1)); // D5 : le plus de lignes = nom canonique
    arr.forEach((f, i) => {
      const day = dayStr(f.d);
      f.canonical = i === 0;
      f.finalName = i === 0 ? `${day}.old.jsonl.gz` : `${day}.${f.host}.old.jsonl.gz`;
      f.finalPath = path.join(f.dir, f.finalName);
    });
    if (arr.length > 1) report.warnings.push(`D5 : ${arr[0].source} ${dayStr(arr[0].d)} a ${arr.length} hôtes (${arr.map((f) => `${f.host}=${f.lines}`).join(', ')}) ; canonique : ${arr[0].host}`);
  }
  for (const f of G.files.values()) {
    const rel = path.relative(G.out, f.finalPath).split(path.sep).join('/');
    const entry = { path: rel, source: f.source, date: dayStr(f.d), host: f.host, canonical: f.canonical, lines: f.lines, lines_by_kind: f.byKind };
    if (!G.dry) {
      await f.close();
      fs.renameSync(f.tmp, f.finalPath);                         // 1) données
      const mpath = f.finalPath.replace(/\.jsonl\.gz$/, '.manifest.json');
      let validatedAt = Date.now() * 1000;
      try { const old = JSON.parse(fs.readFileSync(mpath, 'utf8')); if (old.sha256 === f.hash.copy().digest('hex')) validatedAt = old.validated_at; } catch { /* pas de manifeste précédent */ }
      const man = buildManifest(f, f.finalName, validatedAt);
      fs.writeFileSync(mpath + '.tmp', JSON.stringify(man) + '\n');
      const fd = fs.openSync(mpath + '.tmp', 'r+'); fs.fsyncSync(fd); fs.closeSync(fd);
      fs.renameSync(mpath + '.tmp', mpath);                      // 2) manifeste en dernier = validation
      entry.bytes = man.bytes; entry.sha256 = man.sha256; entry.coverage = man.coverage; entry.gaps = man.gaps.length;
    }
    report.files.push(entry);
  }
  report.files.sort((a, b) => (a.path < b.path ? -1 : 1));
  for (const s of report.sessions) if (s.files) for (const sf of s.files) { // nom final de chaque fichier de session
    const f = [...G.files.values()].find((x) => x.source === sf.source && dayStr(x.d) === sf.date && x.sessions.includes(s.id));
    if (f) sf.path = path.relative(G.out, f.finalPath).split(path.sep).join('/');
  }
}

// ----------------------------------------------------------------------------------------------- programme principal
export async function run({ out, inputs, dry = false, log = console.log }) {
  const report = { tool: 'source-tracing/convert', version: VERSION, generated_at: new Date().toISOString(), dry_run: dry, out: path.resolve(out), inputs, sessions: [], duplicates: [], warnings: [], files: [], totals: {} };
  const files = listInputs(inputs);
  log(`${files.length} fichier(s) de session en entrée`);
  const kept = await plan(files, report);
  const G = { out: path.resolve(out), dry, files: new Map() };
  for (const S of kept) {
    S.text = S.text.endsWith('\r') ? S.text.slice(0, -1) : S.text;
    log(`> ${S.id} (hôte ${S.host}, ${(S.size / 1e6).toFixed(1)} Mo gz)${S.duplicates.length ? ` [${S.duplicates.length} doublon(s) écarté(s)]` : ''}`);
    const R = await convertSession(S, G);
    report.sessions.push(R);
    log(`  lues ${R.lines_read} | converties ${R.check.mapped_in} | écrites(données) ${R.check.data_out} | clock ${R.clock.read}→${R.clock.copies_written} copies | illisibles ${R.unreadable.count} | non converties ${JSON.stringify(R.not_converted)} | tronquée ${R.truncated_input}${R.check.ok ? '' : ' | ÉCART !'}`);
  }
  await finalize(G, report);
  const sum = (f) => report.sessions.reduce((n, s) => n + (f(s) || 0), 0);
  report.totals = {
    sessions_converted: report.sessions.filter((s) => s.status === 'convertie').length,
    lines_read: sum((s) => s.lines_read), mapped_in: sum((s) => s.check?.mapped_in), data_out: sum((s) => s.check?.data_out),
    clock_read: sum((s) => s.clock?.read), clock_copies_written: sum((s) => s.clock?.copies_written),
    recorder_start: sum((s) => s.lifecycle_written?.recorder_start), gap: sum((s) => s.lifecycle_written?.gap),
    unreadable: sum((s) => s.unreadable?.count), files: report.files.length, lines_in_files: report.files.reduce((n, f) => n + f.lines, 0),
  };
  report.totals.conservation_ok = report.totals.mapped_in === report.totals.data_out &&
    report.totals.lines_in_files === report.totals.data_out + report.totals.recorder_start + report.totals.gap + report.totals.clock_copies_written;
  for (const w of report.warnings) log(`! ${w}`);
  log(`${dry ? 'PLAN (rien écrit)' : 'Écrit'} : ${report.totals.files} fichier(s)`);
  for (const f of report.files) log(`  ${f.path}  ${f.lines} lignes${f.canonical ? '' : ' (hôte secondaire, D5)'}${f.bytes ? `  ${f.bytes} o` : ''}`);
  log(`Totaux : ${JSON.stringify(report.totals)}`);
  if (!dry) {
    const rp = path.basename(G.out) === 'trace' ? path.join(G.out, '..', 'convert-report.json') : path.join(G.out, 'convert-report.json');
    fs.writeFileSync(rp + '.tmp', JSON.stringify(report, null, 1) + '\n'); fs.renameSync(rp + '.tmp', rp);
    log(`Rapport : ${rp}`);
  }
  return report;
}

function cli(argv) {
  const a = { inputs: [], dry: false, out: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--out') a.out = argv[++i];
    else if (argv[i] === '--dry-run') a.dry = true;
    else if (argv[i].startsWith('--')) { console.error(`option inconnue : ${argv[i]}`); process.exit(2); }
    else a.inputs.push(argv[i]);
  }
  if (!a.out || !a.inputs.length) { console.error('usage : node convert.mjs --out <racine trace> [--dry-run] <dossier ou fichier de sessions>...'); process.exit(2); }
  return a;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run(cli(process.argv.slice(2))).then((r) => { if (!r.totals.conservation_ok) process.exitCode = 1; }, (e) => { console.error(e); process.exit(1); });
}
