use std::{collections::HashMap, sync::Mutex};

/// Máximo de falhas dentro da janela antes do bloqueio.
const MAX_FAILURES: u32 = 5;
/// Duração da janela (e do bloqueio), em segundos.
const WINDOW_SECS: i64 = 900;

/// Limitador de tentativas de login, por chave (username em minúsculas).
///
/// 5 tentativas não bem-sucedidas dentro de 900 s bloqueiam até 900 s após a
/// primeira tentativa da janela.
#[derive(Debug, Default)]
pub struct LoginLimiter {
    /// chave -> (início da janela, falhas na janela)
    entries: Mutex<HashMap<String, (i64, u32)>>,
}

impl LoginLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pede permissão para uma tentativa de login, de forma atômica: sob o
    /// mutex verifica a janela e já conta a tentativa. `false` = bloqueado
    /// (e a tentativa bloqueada não é contada). Quem loga com sucesso deve
    /// chamar `reset`, o que desfaz a contagem.
    pub fn try_acquire(&self, key: &str, now: i64) -> bool {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        // Descarta janelas vencidas para o mapa não crescer sem limite.
        map.retain(|_, &mut (start, _)| now < start + WINDOW_SECS);
        let entry = map.entry(key.to_string()).or_insert((now, 0));
        if entry.1 >= MAX_FAILURES {
            return false;
        }
        entry.1 += 1;
        true
    }

    pub fn reset(&self, key: &str) {
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(key);
    }
}
