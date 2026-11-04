use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use painel::auth::password::run_limited;

/// Dispara 16 tarefas "pesadas" ao mesmo tempo: nunca mais de 4 rodam juntas.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn argon_concurrency_is_capped_at_four() {
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..16 {
        let (running, peak) = (running.clone(), peak.clone());
        handles.push(tokio::spawn(run_limited(move || {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(60));
            running.fetch_sub(1, Ordering::SeqCst);
        })));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let peak = peak.load(Ordering::SeqCst);
    assert!(peak <= 4, "pico de {peak} execuções simultâneas (máximo 4)");
    assert!(peak >= 2, "o teste precisa exercitar paralelismo (pico {peak})");
}
