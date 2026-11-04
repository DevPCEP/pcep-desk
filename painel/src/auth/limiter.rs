use std::{collections::HashMap, sync::Mutex};

/// Máximo de falhas dentro da janela antes do bloqueio.
const MAX_FAILURES: u32 = 5;
/// Duração da janela (e do bloqueio), em segundos.
const WINDOW_SECS: i64 = 900;

/// Limitador de tentativas de login, por chave (username em minúsculas).
///
/// 5 falhas dentro de 900 s bloqueiam até 900 s após a primeira falha da janela.
#[derive(Debug, Default)]
pub struct LoginLimiter {
    /// chave -> (início da janela, falhas na janela)
    entries: Mutex<HashMap<String, (i64, u32)>>,
}

impl LoginLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// `false` = bloqueado.
    pub fn check(&self, key: &str, now: i64) -> bool {
        let map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some(&(start, count)) => !(count >= MAX_FAILURES && now < start + WINDOW_SECS),
            None => true,
        }
    }

    pub fn record_failure(&self, key: &str, now: i64) {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        // Descarta janelas vencidas para o mapa não crescer sem limite.
        map.retain(|_, &mut (start, _)| now < start + WINDOW_SECS);
        let entry = map.entry(key.to_string()).or_insert((now, 0));
        entry.1 += 1;
    }

    pub fn reset(&self, key: &str) {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(key);
    }
}
