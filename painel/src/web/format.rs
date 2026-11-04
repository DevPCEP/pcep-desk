//! Formatação compartilhada entre as páginas do painel.

/// Código de conexão agrupado de 3 em 3 dígitos ("123 456 789") quando for
/// só dígitos; IDs personalizados ficam como estão.
pub fn group_code(code: &str) -> String {
    if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
        return code.to_string();
    }
    code.as_bytes()
        .chunks(3)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Duração `HH:MM:SS` do aceite até o fim; vazia se faltar algum dos dois.
pub fn duration_label(authorized_at: Option<i64>, ended_at: Option<i64>) -> String {
    match (authorized_at, ended_at) {
        (Some(a), Some(e)) if e >= a => {
            let s = e - a;
            format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_code_by_three_digits() {
        assert_eq!(group_code("123456789"), "123 456 789");
        assert_eq!(group_code("1234567"), "123 456 7");
        assert_eq!(group_code("meu-pc-01"), "meu-pc-01");
        assert_eq!(group_code(""), "");
    }

    #[test]
    fn duration_formats_hms() {
        assert_eq!(duration_label(Some(100), Some(3825)), "01:02:05");
        assert_eq!(duration_label(Some(100), None), "");
        assert_eq!(duration_label(None, Some(100)), "");
        assert_eq!(duration_label(Some(200), Some(100)), "");
    }
}
