#!/usr/bin/env python3
"""UI de copy-trading : serveur local en lecture seule (bibliothèque standard seulement).

Lit le journal du bot sur le serveur par ssh (`tail -F`), le diffuse au navigateur (SSE) et relaie, avec un cache,
les API publiques de Polymarket (marché, carnet, historique de prix, positions du wallet suivi). Le bot n'est jamais
sollicité ; sur le serveur, seule tourne la commande `tail`.

  python copy-trading/ui/serve.py                         journal du service (copy-trading/deploy/.env)
  python copy-trading/ui/serve.py --journal ~/ct-maker/journal.jsonl --no-sudo     un run de test
  python copy-trading/ui/serve.py --file journal.jsonl    un journal local (sans ssh)
"""
import argparse
import concurrent.futures
import json
import os
import queue
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
ENV = HERE.parent / "deploy" / ".env"
STATIC = {"/": ("index.html", "text/html"), "/app.js": ("app.js", "text/javascript"),
          "/model.js": ("model.js", "text/javascript"), "/app.css": ("app.css", "text/css")}


def read_env(path):
    """`deploy/.env` lu ligne à ligne, jamais exécuté (comme deploy/lib.sh)."""
    env = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.split("#", 1)[0].strip()
            if "=" in line:
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip().strip('"').strip("'")
    return env


class Journal:
    """Toutes les lignes reçues, et les abonnés SSE à qui les pousser. Un `reset` vide tout (reconnexion ssh)."""

    def __init__(self):
        self.lines, self.subs, self.lock = [], set(), threading.Lock()
        self.status = "connexion"

    def push(self, kind, data):
        with self.lock:
            if kind == "reset":
                self.lines = []
            elif kind == "line":
                self.lines.append(data)
            for q in list(self.subs):
                q.put((kind, data))

    def subscribe(self):
        q = queue.Queue()
        with self.lock:
            backlog = list(self.lines)
            self.subs.add(q)
        return q, backlog

    def set_status(self, s):
        self.status = s
        self.push("status", s)


def follow(journal, cmd):
    """Relance `cmd` (ssh … tail -F) tant que le serveur tourne ; chaque relance repart du début du journal."""
    while True:
        journal.set_status("connexion")
        try:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
            journal.push("reset", None)
            journal.set_status("ok")
            for raw in p.stdout:
                line = raw.decode("utf-8", "replace").strip()
                if line.startswith("{"):
                    journal.push("line", line)
            p.wait()
        except OSError as e:
            print(f"journal : {e}")
        journal.set_status("coupé")
        time.sleep(5)


def read_file(journal, path):
    """Mode local : relit le fichier, puis suit ses ajouts."""
    journal.push("reset", None)
    journal.set_status("ok")
    with open(path, encoding="utf-8") as f:
        while True:
            line = f.readline()
            if not line:
                time.sleep(1)
                continue
            if line.strip().startswith("{"):
                journal.push("line", line.strip())


CACHE, CACHE_LOCK = {}, threading.Lock()
UPSTREAM = {
    "market": ("https://gamma-api.polymarket.com/markets?clob_token_ids={token}", 86400),
    "book": ("https://clob.polymarket.com/book?token_id={token}", 4),
    "history": ("https://clob.polymarket.com/prices-history?market={token}&startTs={start}&fidelity=1", 60),
    "positions": ("https://data-api.polymarket.com/positions?user={wallet}&sizeThreshold=0.01&limit=500&offset={offset}", 30),
}


def fetch_positions(params):
    """Toutes ses positions : l'API en rend 500 au plus par page (795 le 2026-10-29)."""
    rows = []
    for page in range(6):
        part = json.loads(fetch("positions", {**params, "offset": page * 500}))
        rows += part
        if len(part) < 500:
            break
    return json.dumps(rows).encode()


def fetch(kind, params):
    url_t, ttl = UPSTREAM[kind]
    url = url_t.format(**{k: urllib.parse.quote(str(v)) for k, v in params.items()})
    now = time.time()
    with CACHE_LOCK:
        hit = CACHE.get(url)
        if hit and now - hit[0] < ttl:
            return hit[1]
    req = urllib.request.Request(url, headers={"User-Agent": "copy-trading-ui"})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=10) as r:
                body = r.read()
            break
        except urllib.error.HTTPError as e:
            # limite de débit ou panne passagère de l'amont : deux nouvelles tentatives espacées
            if e.code not in (429, 500, 502, 503, 504) or attempt == 2:
                raise
            time.sleep(1 + attempt)
    if kind == "market":
        body = json.dumps(market_meta(json.loads(body), params["token"])).encode()
    with CACHE_LOCK:
        CACHE[url] = (now, body)
        if len(CACHE) > 2000:
            CACHE.clear()
    return body


def market_meta(rows, token):
    """Ce que l'UI montre d'un marché : question, intitulé court, issue de CE token, icône, fin."""
    if not rows:
        return {}
    m = rows[0]
    ids, outs = json.loads(m.get("clobTokenIds") or "[]"), json.loads(m.get("outcomes") or "[]")
    outcome = outs[ids.index(token)] if token in ids and len(outs) == len(ids) else None
    return {"question": m.get("question"), "title": m.get("groupItemTitle") or m.get("question"),
            "outcome": outcome, "icon": m.get("icon"), "end": m.get("endDate"), "slug": m.get("slug")}


def safe_json(f):
    try:
        return json.loads(f())
    except Exception:
        return None


def make_handler(journal, wallet):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def send(self, code, ctype, body):
            self.send_response(code)
            self.send_header("Content-Type", ctype + "; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            # contre le DNS rebinding : seule une page servie par 127.0.0.1 / localhost lit le journal
            if self.headers.get("Host", "").rsplit(":", 1)[0] not in ("127.0.0.1", "localhost"):
                return self.send(403, "text/plain", b"hote refuse")
            u = urllib.parse.urlparse(self.path)
            q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
            if u.path in STATIC:
                name, ctype = STATIC[u.path]
                return self.send(200, ctype, (HERE / name).read_bytes())
            if u.path == "/events":
                return self.events()
            if u.path == "/api/markets":
                # tous les marchés demandés en une requête : le navigateur ne tient que 6 connexions, dont le flux
                toks = [t for t in q.get("tokens", "").split(",") if t.isdigit()][:40]
                with concurrent.futures.ThreadPoolExecutor(4) as ex:
                    res = dict(zip(toks, ex.map(lambda t: safe_json(lambda: fetch("market", {"token": t})), toks)))
                return self.send(200, "application/json", json.dumps(res).encode())
            if u.path.startswith("/api/"):
                kind = u.path[5:]
                if kind not in UPSTREAM:
                    return self.send(404, "text/plain", b"inconnu")
                params = {"wallet": q.get("wallet") or wallet, "token": q.get("token", ""), "start": q.get("start", "0")}
                # un token CLOB est un entier décimal, une adresse 0x + 40 hex : rien d'autre ne part vers l'amont
                if kind != "positions" and not params["token"].isdigit():
                    return self.send(400, "text/plain", b"token invalide")
                if kind == "positions" and not (len(params["wallet"]) == 42 and params["wallet"].startswith("0x")):
                    return self.send(400, "text/plain", b"wallet invalide")
                if not params["start"].isdigit():
                    return self.send(400, "text/plain", b"start invalide")
                try:
                    body = fetch_positions(params) if kind == "positions" else fetch(kind, params)
                    return self.send(200, "application/json", body)
                except Exception as e:  # l'amont est indisponible : l'UI garde sa dernière valeur
                    return self.send(502, "application/json", json.dumps({"error": str(e)}).encode())
            self.send(404, "text/plain", b"introuvable")

        def events(self):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            q, backlog = journal.subscribe()
            try:
                self.wfile.write(b"event: reset\ndata: {}\n\n")
                self.wfile.write(f"event: status\ndata: {json.dumps(journal.status)}\n\n".encode())
                # le passé en un seul bloc : le navigateur l'ingère d'un coup puis passe au direct
                self.wfile.write(b"event: batch\ndata: [" + ",".join(backlog).encode() + b"]\n\n")
                self.wfile.flush()
                while True:
                    try:
                        kind, data = q.get(timeout=15)
                    except queue.Empty:
                        self.wfile.write(b": keepalive\n\n")
                        self.wfile.flush()
                        continue
                    payload = data if kind == "line" else json.dumps(data)
                    self.wfile.write(f"event: {kind}\ndata: {payload}\n\n".encode())
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            finally:
                with journal.lock:
                    journal.subs.discard(q)

    return H


def main():
    env = read_env(ENV)
    ap = argparse.ArgumentParser(description="UI de copy-trading (lecture seule)")
    ap.add_argument("--journal", default="/var/lib/copy-trading/journal.jsonl", help="chemin du journal sur le serveur")
    ap.add_argument("--no-sudo", action="store_true", help="lire le journal en tant qu'ubuntu (run de test hors service)")
    ap.add_argument("--file", help="journal local (sans ssh)")
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()
    journal = Journal()
    if a.file:
        threading.Thread(target=read_file, args=(journal, a.file), daemon=True).start()
    else:
        dest, key = env.get("COPY_SSH_DEST"), os.path.expanduser(env.get("COPY_SSH_KEY", ""))
        if not dest or not key:
            raise SystemExit(f"COPY_SSH_DEST et COPY_SSH_KEY manquent dans {ENV}")
        path = a.journal.replace("'", "")
        tail = f"tail -n +1 -F '{path}'" if a.no_sudo else f"sudo -n -u copyt tail -n +1 -F '{path}'"
        cmd = ["ssh", "-i", key, "-o", "BatchMode=yes", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3", dest, tail]
        threading.Thread(target=follow, args=(journal, cmd), daemon=True).start()
    if env.get("COPY_WALLET"):
        # ses positions (2 pages) sont lentes à froid : on chauffe le cache pendant que la page s'ouvre
        threading.Thread(target=safe_json, args=(lambda: fetch_positions({"wallet": env["COPY_WALLET"], "token": "", "start": "0"}),), daemon=True).start()
    srv = ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(journal, env.get("COPY_WALLET", "")))
    srv.daemon_threads = True
    print(f"UI copy-trading : http://127.0.0.1:{a.port}/  (Ctrl+C pour arrêter)")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
