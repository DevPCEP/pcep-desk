use std::time::{SystemTime, UNIX_EPOCH};

use time::{OffsetDateTime, UtcOffset};

/// Segundos Unix (UTC) atuais.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Formata um timestamp Unix como `dd/mm/aaaa HH:MM` no fuso fixo UTC−03:00.
pub fn format_ts(ts: i64) -> String {
    let brt = UtcOffset::from_hms(-3, 0, 0).expect("offset válido");
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => {
            let dt = dt.to_offset(brt);
            format!(
                "{:02}/{:02}/{:04} {:02}:{:02}",
                dt.day(),
                u8::from(dt.month()),
                dt.year(),
                dt.hour(),
                dt.minute()
            )
        }
        Err(_) => String::new(),
    }
}
