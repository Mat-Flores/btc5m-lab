//! Construction JSON dans l'ordre exact des clés (serde_json trie les clés d'un objet : on écrit à la main).
use serde::Serialize;

pub struct Obj(String);

impl Obj {
    pub fn new() -> Obj { Obj("{".into()) }
    fn sep(&mut self, k: &str) {
        if self.0.len() > 1 { self.0.push(','); }
        self.0.push('"'); self.0.push_str(k); self.0.push_str("\":");
    }
    pub fn v<T: Serialize + ?Sized>(mut self, k: &str, x: &T) -> Obj { self.sep(k); self.0.push_str(&js(x)); self }
    pub fn raw(mut self, k: &str, json: &str) -> Obj { self.sep(k); self.0.push_str(json); self }
    pub fn end(mut self) -> String { self.0.push('}'); self.0 }
}

pub fn js<T: Serialize + ?Sized>(x: &T) -> String { serde_json::to_string(x).expect("serialisation JSON") }
