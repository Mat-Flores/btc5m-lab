//! Minimal HTTP. `get`: one HTTPS GET per connection, for the cold thread (metadata, resolutions). `Conn`: one kept-alive
//! connection that writes prepared requests and reads their answers, for the order path (warm, no connection set-up
//! on the hot path). Identity encoding; fixed-length or chunked bodies.
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

#[derive(Debug)]
pub enum HttpError { Url, Io(String), Status(u16), Malformed(&'static str), Timeout }

fn tls() -> TlsConnector {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let cfg = CFG.get_or_init(|| {
        let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
    });
    TlsConnector::from(cfg.clone())
}

/// GET `url` (https only) and return the body of a 200 answer.
pub async fn get(url: &str, timeout: Duration) -> Result<Vec<u8>, HttpError> {
    tokio::time::timeout(timeout, get_inner(url)).await.map_err(|_| HttpError::Timeout)?
}

async fn get_inner(url: &str) -> Result<Vec<u8>, HttpError> {
    let rest = url.strip_prefix("https://").ok_or(HttpError::Url)?;
    let (host, path) = match rest.find('/') { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "/") };
    let io = |e: std::io::Error| HttpError::Io(e.to_string());
    let tcp = TcpStream::connect((host, 443)).await.map_err(io)?;
    tcp.set_nodelay(true).map_err(io)?;
    let name = ServerName::try_from(host.to_string()).map_err(|_| HttpError::Url)?;
    let mut s = tls().connect(name, tcp).await.map_err(io)?;
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: engine\r\nAccept: application/json\r\nConnection: close\r\n\r\n").as_bytes()).await.map_err(io)?;
    let mut raw = Vec::new();
    // a server may end the TLS session without close_notify after Connection: close; what was read is kept
    if let Err(e) = s.read_to_end(&mut raw).await { if raw.is_empty() { return Err(io(e)); } }
    parse(&raw)
}

fn parse(raw: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut res = httparse::Response::new(&mut headers);
    let head = match res.parse(raw).map_err(|_| HttpError::Malformed("head"))? { httparse::Status::Complete(n) => n, _ => return Err(HttpError::Malformed("head")) };
    let code = res.code.unwrap_or(0);
    if code != 200 { return Err(HttpError::Status(code)); }
    let h = |name: &str| res.headers.iter().find(|x| x.name.eq_ignore_ascii_case(name)).map(|x| String::from_utf8_lossy(x.value).to_ascii_lowercase());
    let body = &raw[head..];
    if h("transfer-encoding").is_some_and(|v| v.contains("chunked")) { return dechunk(body); }
    match h("content-length").and_then(|v| v.trim().parse::<usize>().ok()) {
        Some(n) if body.len() >= n => Ok(body[..n].to_vec()),
        Some(_) => Err(HttpError::Malformed("short body")),
        None => Ok(body.to_vec()),
    }
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut out = Vec::new();
    loop {
        let nl = b.windows(2).position(|w| w == b"\r\n").ok_or(HttpError::Malformed("chunk size"))?;
        let size_txt = std::str::from_utf8(&b[..nl]).map_err(|_| HttpError::Malformed("chunk size"))?;
        let size = usize::from_str_radix(size_txt.split(';').next().unwrap_or("").trim(), 16).map_err(|_| HttpError::Malformed("chunk size"))?;
        b = &b[nl + 2..];
        if size == 0 { return Ok(out); }
        if size > MAX_BODY || b.len() < size + 2 { return Err(HttpError::Malformed("short chunk")); }
        out.extend_from_slice(&b[..size]);
        b = &b[size + 2..];
    }
}

/// A byte stream: TLS for a venue, plain TCP for a local test server.
pub trait Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Io for T {}

/// Opens a connection to `host:port`, TLS when `tls`.
pub async fn dial(host: &str, port: u16, tls_on: bool) -> Result<Box<dyn Io>, HttpError> {
    let io = |e: std::io::Error| HttpError::Io(e.to_string());
    let tcp = TcpStream::connect((host, port)).await.map_err(io)?;
    tcp.set_nodelay(true).map_err(io)?;
    if !tls_on { return Ok(Box::new(tcp)); }
    let name = ServerName::try_from(host.to_string()).map_err(|_| HttpError::Url)?;
    Ok(Box::new(tls().connect(name, tcp).await.map_err(io)?))
}

#[derive(Debug, PartialEq)]
pub struct Response { pub status: u16, pub body: Vec<u8>, /// Seconds the server asks to wait (Retry-After).
    pub retry_after_s: Option<u64>, pub close: bool }

/// One kept-alive HTTP/1.1 connection: requests are written whole, answers read in order.
pub struct Conn { io: Box<dyn Io>, buf: Vec<u8> }

impl Conn {
    pub fn new(io: Box<dyn Io>) -> Self { Conn { io, buf: Vec::with_capacity(16 * 1024) } }

    /// Writes a whole request. Returns once its last byte is handed to the socket.
    pub async fn write(&mut self, req: &[u8]) -> Result<(), HttpError> {
        let io = |e: std::io::Error| HttpError::Io(e.to_string());
        self.io.write_all(req).await.map_err(io)?;
        self.io.flush().await.map_err(io)
    }

    /// Reads the next answer whole. An end of stream before it is complete is an error (the answer is lost).
    pub async fn read(&mut self) -> Result<Response, HttpError> {
        loop {
            if let Some((r, used)) = take(&self.buf)? { self.buf.drain(..used); return Ok(r); }
            let n = self.io.read_buf(&mut self.buf).await.map_err(|e| HttpError::Io(e.to_string()))?;
            if n == 0 { return Err(HttpError::Io("closed before a whole answer".into())); }
            if self.buf.len() > 2 * MAX_BODY { return Err(HttpError::Malformed("answer too large")); }
        }
    }
}

/// One whole answer at the start of `raw`, and the bytes it used; None while incomplete.
fn take(raw: &[u8]) -> Result<Option<(Response, usize)>, HttpError> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut res = httparse::Response::new(&mut headers);
    let head = match res.parse(raw).map_err(|_| HttpError::Malformed("head"))? { httparse::Status::Complete(n) => n, httparse::Status::Partial => return Ok(None) };
    let h = |name: &str| res.headers.iter().find(|x| x.name.eq_ignore_ascii_case(name)).map(|x| String::from_utf8_lossy(x.value).trim().to_ascii_lowercase());
    let (status, retry_after_s, close) = (res.code.unwrap_or(0), h("retry-after").and_then(|v| v.parse().ok()), h("connection").is_some_and(|v| v == "close"));
    let rest = &raw[head..];
    let (body, used) = if h("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
        match chunks(rest)? { Some((body, n)) => (body, head + n), None => return Ok(None) }
    } else {
        let n = h("content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        if n > MAX_BODY { return Err(HttpError::Malformed("body too large")); }
        if rest.len() < n { return Ok(None); }
        (rest[..n].to_vec(), head + n)
    };
    Ok(Some((Response { status, body, retry_after_s, close }, used)))
}

/// Largest answer body accepted on a kept-alive connection (an order answer is a few hundred bytes).
const MAX_BODY: usize = 1 << 20;

/// Walks a chunked body: (body, bytes used) once the last chunk and its final line are in, None before.
fn chunks(b: &[u8]) -> Result<Option<(Vec<u8>, usize)>, HttpError> {
    let (mut out, mut at) = (Vec::new(), 0);
    loop {
        let Some(nl) = b[at..].windows(2).position(|w| w == b"\r\n") else { return Ok(None) };
        let size_txt = std::str::from_utf8(&b[at..at + nl]).map_err(|_| HttpError::Malformed("chunk size"))?;
        let size = usize::from_str_radix(size_txt.split(';').next().unwrap_or("").trim(), 16).map_err(|_| HttpError::Malformed("chunk size"))?;
        at += nl + 2;
        // a size from the network is never trusted: bounded before any offset is computed from it
        if size > MAX_BODY || out.len() + size > MAX_BODY { return Err(HttpError::Malformed("body too large")); }
        if b.len() < at + size + 2 { return Ok(None); }
        if &b[at + size..at + size + 2] != b"\r\n" { return Err(HttpError::Malformed("chunk end")); }
        out.extend_from_slice(&b[at..at + size]);
        at += size + 2;
        if size == 0 { return Ok(Some((out, at))); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_length_chunked_and_errors() {
        assert_eq!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").unwrap(), b"hello");
        assert_eq!(parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6; x=1\r\n world\r\n0\r\n\r\n").unwrap(), b"hello world");
        assert!(matches!(parse(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"), Err(HttpError::Status(404))));
        assert!(matches!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello"), Err(HttpError::Malformed(_))));
        assert!(matches!(parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel"), Err(HttpError::Malformed(_))));
        assert!(parse(b"garbage").is_err());
    }

    #[test]
    fn answers_are_taken_whole_and_in_order() {
        let two = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nokHTTP/1.1 429 Too Many\r\nRetry-After: 3\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nno\r\n0\r\n\r\n";
        let (a, n) = take(two).unwrap().unwrap();
        assert_eq!((a.status, a.body.as_slice(), a.retry_after_s), (200, &b"ok"[..], None));
        let (b, m) = take(&two[n..]).unwrap().unwrap();
        assert_eq!((b.status, b.body.as_slice(), b.retry_after_s, n + m), (429, &b"no"[..], Some(3), two.len()));
        // every strict prefix is incomplete, never a wrong answer
        for i in 0..n { assert!(take(&two[..i]).unwrap().is_none(), "{i}"); }
        // a chunk whose data looks like the end does not end the body
        let tricky = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n0\r\n\r\n\r\n0\r\n\r\n";
        assert_eq!(take(tricky).unwrap().unwrap().0.body, b"0\r\n\r\n");
        // sizes from the network cannot overflow offsets nor grow the buffer without bound
        assert!(take(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nx").is_err());
        assert!(take(b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999\r\n\r\n").is_err());
        assert!(take(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap().unwrap().0.close);
    }
}
