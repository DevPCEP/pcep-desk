use anyhow::{Context, bail};

/// Configuração lida das variáveis de ambiente.
#[derive(Debug, Clone)]
pub struct Config {
    pub db_url: String,
    pub bind: String,
    pub public_url: String,
    pub cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Separado de `from_env` para permitir teste sem mexer no ambiente do processo.
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config> {
        let db_url = get("PAINEL_DB").unwrap_or_else(|| "sqlite://dados/painel.db".to_string());
        let bind = get("PAINEL_ENDERECO").unwrap_or_else(|| "0.0.0.0:8080".to_string());
        let public_url = get("PAINEL_URL_PUBLICA")
            .filter(|v| !v.trim().is_empty())
            .context("variável PAINEL_URL_PUBLICA é obrigatória (ex.: https://desk.pcepcontabil.com.br)")?;
        let public_url = public_url.trim().trim_end_matches('/').to_string();
        let cookie_secure = match get("PAINEL_COOKIE_SEGURO") {
            None => true,
            Some(v) => match v.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "sim" => true,
                "false" | "0" | "nao" | "não" => false,
                other => bail!("PAINEL_COOKIE_SEGURO inválida: {other:?} (use true ou false)"),
            },
        };
        Ok(Config {
            db_url,
            bind,
            public_url,
            cookie_secure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn defaults_and_required_url() {
        assert!(Config::from_lookup(lookup(&[])).is_err());
        let c = Config::from_lookup(lookup(&[("PAINEL_URL_PUBLICA", "https://x.com/")])).unwrap();
        assert_eq!(c.db_url, "sqlite://dados/painel.db");
        assert_eq!(c.bind, "0.0.0.0:8080");
        assert_eq!(c.public_url, "https://x.com");
        assert!(c.cookie_secure);
    }

    #[test]
    fn cookie_secure_parsing() {
        let c = Config::from_lookup(lookup(&[
            ("PAINEL_URL_PUBLICA", "http://l"),
            ("PAINEL_COOKIE_SEGURO", "false"),
        ]))
        .unwrap();
        assert!(!c.cookie_secure);
        assert!(
            Config::from_lookup(lookup(&[
                ("PAINEL_URL_PUBLICA", "http://l"),
                ("PAINEL_COOKIE_SEGURO", "talvez"),
            ]))
            .is_err()
        );
    }
}
