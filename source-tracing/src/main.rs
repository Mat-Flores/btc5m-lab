//! source-tracing : enregistreur continu des flux (Contrat 1 : format de trace, Contrat 3 : status/config).
//! Usage : `source-tracing [--config <config.json>]` (défaut : `config.json` du dossier courant).
mod config;
mod http;
mod json;
mod run;
mod seal;
mod source;
#[cfg(test)]
mod tests;
mod util;
mod writer;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path = "config.json".to_string();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => path = args.next().unwrap_or_else(|| { eprintln!("--config demande un chemin"); std::process::exit(2) }),
            _ => { eprintln!("usage : source-tracing [--config <config.json>]"); std::process::exit(2) }
        }
    }
    // un seul fil : les écritures sont courtes (gzip est un sous-processus)
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime tokio");
    if let Err(e) = rt.block_on(run::run(path)) { eprintln!("{e}"); std::process::exit(2); }
}
