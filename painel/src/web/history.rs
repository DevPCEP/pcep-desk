//! Histórico de conexões: quem do escritório acessou qual cliente, quando e
//! por quanto tempo.

use askama::Template;
use askama_web::WebTemplate;
use axum::{
    Router,
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

use crate::{
    clients, clock,
    connections::{self, HistoryFilter, HistoryRow},
    state::AppState,
    users::{self, User},
    web::{
        format::{duration_label, group_code},
        session::WebUser,
        users::internal,
    },
};

const ACTIVE: &str = "historico";
/// Máximo de conexões exibidas.
const LIMIT: i64 = 500;

pub fn router() -> Router<AppState> {
    Router::new().route("/historico", get(page))
}

/// Parâmetros do filtro; tudo texto para aceitar campos vazios ou malformados
/// sem erro de formato.
#[derive(Deserialize, Default)]
#[serde(default)]
struct HistoryQuery {
    colaborador: String,
    cliente: String,
    de: String,
    ate: String,
}

struct SelectOption {
    id: i64,
    label: String,
    selected: bool,
}

struct Row {
    started: String,
    operator: String,
    computer: String,
    kind: &'static str,
    status: &'static str,
    duration: String,
    ip: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "historico.html")]
struct HistoryPage {
    user: User,
    active: &'static str,
    /// `Some` só para administrador (colaborador vê apenas o próprio).
    users: Option<Vec<SelectOption>>,
    clients: Vec<SelectOption>,
    de: String,
    ate: String,
    date_error: bool,
    rows: Vec<Row>,
    truncated: bool,
}

/// Id numérico do filtro; vazio ou inválido = sem filtro.
fn parse_id(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok()
}

/// `aaaa-mm-dd` estrito.
fn parse_date(s: &str) -> Option<Date> {
    let mut parts = s.trim().split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let all_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if !(all_digits(y) && all_digits(m) && all_digits(d)) {
        return None;
    }
    let month = Month::try_from(m.parse::<u8>().ok()?).ok()?;
    Date::from_calendar_date(y.parse().ok()?, month, d.parse().ok()?).ok()
}

/// Meia-noite do dia em UTC−03:00, em segundos Unix.
fn midnight_brt(date: Date) -> i64 {
    let offset = UtcOffset::from_hms(-3, 0, 0).expect("offset válido");
    PrimitiveDateTime::new(date, Time::MIDNIGHT)
        .assume_offset(offset)
        .unix_timestamp()
}

/// Intervalo `[from, to)` em segundos Unix; `ate` é inclusivo, então `to` é a
/// meia-noite do dia seguinte. `bool`: houve data inválida (filtro ignorado).
fn parse_period(de: &str, ate: &str) -> (Option<i64>, Option<i64>, bool) {
    let mut invalid = false;
    let mut bound = |s: &str, end: bool| {
        if s.trim().is_empty() {
            return None;
        }
        let date = parse_date(s).and_then(|d| if end { d.next_day() } else { Some(d) });
        if date.is_none() {
            invalid = true;
        }
        date.map(midnight_brt)
    };
    let from = bound(de, false);
    let to = bound(ate, true);
    (from, to, invalid)
}

fn row(r: HistoryRow) -> Row {
    let status = connections::status_label(&r);
    let operator = match r.user_display_name {
        Some(name) => name,
        None if r.operator_name.is_empty() => "—".to_string(),
        None => format!("{} (não vinculado)", r.operator_name),
    };
    let computer = match (&r.client_name, &r.computer_label) {
        (Some(client), Some(label)) => format!("{client} – {label}"),
        _ => group_code(&r.peer_id),
    };
    Row {
        started: clock::format_ts(r.started_at),
        operator,
        computer,
        kind: connections::type_label(r.conn_type),
        status,
        duration: duration_label(r.authorized_at, r.ended_at),
        ip: r.ip,
    }
}

async fn page(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, Response> {
    let (from, to, date_error) = parse_period(&query.de, &query.ate);
    // Colaborador comum só enxerga as próprias conexões.
    let user_id = if user.is_admin {
        parse_id(&query.colaborador)
    } else {
        Some(user.id)
    };
    let client_id = parse_id(&query.cliente);

    let mut found = connections::query(
        &st.pool,
        &HistoryFilter {
            user_id,
            client_id,
            from,
            to,
            limit: LIMIT + 1,
        },
    )
    .await
    .map_err(internal)?;
    let truncated = found.len() as i64 > LIMIT;
    found.truncate(LIMIT as usize);

    let users = if user.is_admin {
        let all = users::list(&st.pool).await.map_err(internal)?;
        Some(
            all.into_iter()
                .map(|u| SelectOption {
                    selected: Some(u.id) == user_id,
                    id: u.id,
                    label: u.display_name,
                })
                .collect(),
        )
    } else {
        None
    };
    let clients = clients::search(&st.pool, "", i64::MAX)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| SelectOption {
            selected: Some(c.id) == client_id,
            id: c.id,
            label: c.name,
        })
        .collect();

    Ok(HistoryPage {
        user,
        active: ACTIVE,
        users,
        clients,
        de: query.de,
        ate: query.ate,
        date_error,
        rows: found.into_iter().map(row).collect(),
        truncated,
    }
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_dates() {
        assert!(parse_date("2026-03-10").is_some());
        assert!(parse_date("2026-02-30").is_none());
        assert!(parse_date("10/03/2026").is_none());
        assert!(parse_date("2026-3-1").is_none());
        assert!(parse_date("+026-03-10").is_none());
        assert!(parse_date("").is_none());
    }

    #[test]
    fn period_end_is_next_midnight_brt() {
        // 2026-03-10 00:00 BRT = 03:00 UTC.
        let (from, to, invalid) = parse_period("2026-03-10", "2026-03-10");
        assert!(!invalid);
        let from = from.unwrap();
        assert_eq!(to.unwrap() - from, 86_400);
        assert_eq!(from % 86_400, 3 * 3600);
    }

    #[test]
    fn invalid_or_empty_period() {
        assert_eq!(parse_period("", ""), (None, None, false));
        let (from, to, invalid) = parse_period("xx", "2026-03-10");
        assert!(invalid && from.is_none() && to.is_some());
    }
}
