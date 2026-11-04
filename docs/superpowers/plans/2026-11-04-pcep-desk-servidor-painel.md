# PCEP Desk — Plano 1: Servidor e Painel

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Servidor próprio (hbbs/hbbr) + painel em Rust que implementa a API HTTP do RustDesk (login, heartbeat, histórico, catálogo de endereços) e uma interface web para colaboradores, clientes/computadores e histórico — testável com o app RustDesk original.

**Architecture:** Um único executável Rust (`painel`) com Axum: rotas `/api/*` compatíveis com o app RustDesk e rotas web (HTML renderizado no servidor com Askama + htmx) para a interface. SQLite via sqlx (consultas em tempo de execução, sem macros que exigem `DATABASE_URL`). Docker Compose junta `hbbs`, `hbbr`, `painel` e Caddy (HTTPS).

**Tech Stack:** Rust 1.95 (edition 2024), axum 0.8, tokio 1, sqlx 0.9 (sqlite, runtime-tokio), askama 0.16 + askama_web 0.16 (feature axum), argon2 0.6, tower-http 0.7, serde/serde_json, time 0.3, rand 0.10, sha2, base64, uuid 1, htmx 2 (arquivo estático). rustdesk-server 1.1.16 (imagem `rustdesk/rustdesk-server`), Caddy 2.

**Spec:** `docs/superpowers/specs/2026-11-04-pcep-desk-design.md`

**Fora deste plano (Plano 2 — fork dos apps):** rebatizar e compilar os apps cliente/colaborador, travar aceite manual, bloquear conexão sem login, desligar verificação de atualização do upstream, registrar tentativas a partir do app do colaborador, página de download no painel (depende dos releases do fork).

## Global Constraints

- Idioma: toda interface, mensagens de erro da API mostradas ao usuário e documentação em português do Brasil. Identificadores no código em inglês (padrão Rust/RustDesk); comentários em português.
- Contrato da API: respostas de erro JSON no formato `{"error": "<mensagem>"}`. Exceções exigidas pelo app: `/api/audit/*` responde **200 com corpo vazio** no sucesso; `/api/sysinfo` responde **texto puro** `SYSINFO_UPDATED`; `/api/sysinfo_ver` responde texto puro; ações do catálogo (`peer/add`, `peer/update`, `peer` DELETE, `tag/*`) respondem **200 com corpo vazio** no sucesso.
- O app trata `/api/currentUser` com status **401** como sessão inválida (desloga). Token revogado/expirado/usuário desativado → 401.
- Paginação: query `current` (1-based) e `pageSize` (limite 100); resposta `{"total": n, "data": [...]}`.
- Senhas com Argon2id (padrão do crate `argon2`). Tokens: 32 bytes aleatórios em base64url sem padding; no banco só o SHA-256 em hex.
- Token do app (`kind = 'app'`) expira em 180 dias; sessão web (`kind = 'web'`) em 12 horas.
- Horários gravados como inteiro Unix (segundos, UTC); exibidos em `dd/mm/aaaa HH:MM` no fuso fixo UTC−03:00.
- Limite de corpo de requisição: 64 KiB em todas as rotas `/api/*`.
- Nome do catálogo compartilhado: `Clientes PCEP`; guid fixo `clientes-pcep`; `rule = 1` (somente leitura) para todos no app. A gestão é pelo painel.
- Variáveis de ambiente (com padrão): `PAINEL_DB` (`sqlite://dados/painel.db`), `PAINEL_ENDERECO` (`0.0.0.0:8080`), `PAINEL_URL_PUBLICA` (obrigatória, ex.: `https://desk.pcepcontabil.com.br`), `PAINEL_COOKIE_SEGURO` (`true`).
- Commits: o diretório ainda não é repositório git; a Task 1 cria. **Cada commit só é feito com autorização explícita do usuário** (regra global). Sem autorização, deixe as mudanças no working tree e siga.
- Entrega crítica (autenticação e permissões): ao final, revisão pelo `auditor` antes de reportar como concluído.

## Review Focus

1. **Colaborador desativado com token salvo no app** → `/api/currentUser`, `/api/ab/*` e páginas web respondem 401/redirect imediatamente após a desativação. Teste na Task 3 (`disabled_user_token_rejected`) e na Task 8 (`disable_revokes_web_and_app`).
2. **Eventos de histórico repetidos ou fora de ordem** (o app reenvia até 3x com o mesmo `nonce`; o evento "autorizado" pode chegar antes do "new") → um único registro de conexão, sem duplicata. Teste na Task 5 (`duplicate_nonce_ignored`, `authorized_before_new_creates_single_record`).
3. **Código de conexão digitado com espaços ou formatação** (`"123 456 789"`) e o mesmo código cadastrado em dois clientes → código normalizado (só dígitos); segunda inclusão mostra erro amigável. Teste na Task 9 (`peer_id_normalized`, `duplicate_peer_id_rejected`).
4. **Tentativas repetidas de senha** → bloqueio após 5 falhas em 15 minutos por usuário (sem diferenciar maiúsculas), inclusive com a senha correta durante o bloqueio; vale para app e web. Teste na Task 3 (`login_lockout_after_five_failures`).
5. **Corpo malformado ou grande em rota sem autenticação** (`heartbeat`, `sysinfo`, `audit`) → 400/413 sem derrubar o processo e sem gravar nada. Teste na Task 4 (`heartbeat_rejects_garbage_and_oversize`) e na Task 5 (`audit_rejects_garbage`).

---

## Estrutura de arquivos

```
pcep-desk/
  .gitignore
  painel/
    Cargo.toml
    migrations/0001_inicial.sql
    src/
      main.rs            CLI: `serve` (padrão) e `criar-admin`
      lib.rs             build_router(AppState) -> Router; mods
      config.rs          Config::from_env
      db.rs              connect(url) -> SqlitePool (cria arquivo, roda migrations)
      state.rs           AppState
      error.rs           ApiError (JSON {"error"}), WebError
      clock.rs           now() -> i64; format_ts(i64) -> String
      auth/mod.rs
      auth/password.rs   hash_password, verify_password
      auth/tokens.rs     issue_token, resolve_token, revoke_user_tokens, revoke_token
      auth/limiter.rs    LoginLimiter
      users.rs           User + repositório
      devices.rs         dispositivos (heartbeat/sysinfo) + consultas de online/não vinculados
      clients.rs         clientes e computadores + normalize_peer_id
      connections.rs     processamento de audit e consultas de histórico
      addressbook.rs     catálogo pessoal (ab_peers/ab_tags) + visão compartilhada
      api/mod.rs         router /api
      api/extract.rs     ApiUser (Bearer)
      api/account.rs     login, currentUser, logout, login-options
      api/devices.rs     heartbeat, sysinfo, sysinfo_ver
      api/audit.rs       audit/conn, audit/file, audit/alarm
      api/ab.rs          catálogo de endereços
      api/groups.rs      users, peers, device-group/accessible
      web/mod.rs         router web
      web/session.rs     WebUser, AdminUser (cookie), checagem de Origin
      web/login.rs       /entrar, /sair
      web/users.rs       /colaboradores
      web/clients.rs     /clientes, /nao-vinculados
      web/history.rs     /historico
    templates/           base.html, entrar.html, colaboradores*.html, clientes*.html, historico.html, nao_vinculados.html
    static/              htmx.min.js, painel.css
    tests/
      common/mod.rs      TestApp e helpers
      account.rs devices.rs audit.rs ab.rs groups.rs web_auth.rs web_users.rs web_clients.rs web_history.rs
  servidor/
    docker-compose.yml  Caddyfile  .env.example  backup.sh  README.md
```

## Esquema do banco (Task 1 cria inteiro)

```sql
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name TEXT NOT NULL,
  email TEXT NOT NULL DEFAULT '',
  password_hash TEXT NOT NULL,
  is_admin INTEGER NOT NULL DEFAULT 0,
  active INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL
);
CREATE TABLE tokens (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL CHECK (kind IN ('app','web')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
-- computadores dos colaboradores (aprendidos no login do app)
CREATE TABLE user_devices (
  peer_id TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  uuid TEXT NOT NULL DEFAULT '',
  last_login_at INTEGER NOT NULL
);
-- todo app que manda heartbeat/sysinfo
CREATE TABLE devices (
  peer_id TEXT PRIMARY KEY,
  uuid TEXT NOT NULL DEFAULT '',
  version TEXT NOT NULL DEFAULT '',
  hostname TEXT NOT NULL DEFAULT '',
  os TEXT NOT NULL DEFAULT '',
  username TEXT NOT NULL DEFAULT '',
  cpu TEXT NOT NULL DEFAULT '',
  memory TEXT NOT NULL DEFAULT '',
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL
);
CREATE TABLE clients (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  document TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE computers (
  id INTEGER PRIMARY KEY,
  client_id INTEGER NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
  peer_id TEXT NOT NULL UNIQUE,
  label TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE ab_personal (
  guid TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE
);
CREATE TABLE ab_peers (
  ab_guid TEXT NOT NULL REFERENCES ab_personal(guid) ON DELETE CASCADE,
  peer_id TEXT NOT NULL,
  alias TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',      -- JSON array de strings
  username TEXT NOT NULL DEFAULT '',
  hostname TEXT NOT NULL DEFAULT '',
  platform TEXT NOT NULL DEFAULT '',
  hash TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (ab_guid, peer_id)
);
CREATE TABLE ab_tags (
  ab_guid TEXT NOT NULL REFERENCES ab_personal(guid) ON DELETE CASCADE,
  name TEXT NOT NULL,
  color INTEGER NOT NULL,
  PRIMARY KEY (ab_guid, name)
);
CREATE TABLE connections (
  id INTEGER PRIMARY KEY,
  peer_id TEXT NOT NULL,            -- computador que RECEBEU (cliente)
  conn_id INTEGER NOT NULL,
  session_id TEXT NOT NULL DEFAULT '',
  ip TEXT NOT NULL DEFAULT '',
  operator_peer_id TEXT NOT NULL DEFAULT '',
  operator_name TEXT NOT NULL DEFAULT '',
  user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
  conn_type INTEGER,
  started_at INTEGER NOT NULL,
  authorized_at INTEGER,
  ended_at INTEGER
);
CREATE INDEX connections_open ON connections(peer_id, conn_id, ended_at);
CREATE INDEX connections_started ON connections(started_at);
CREATE TABLE audit_raw (
  id INTEGER PRIMARY KEY,
  nonce TEXT UNIQUE,
  kind TEXT NOT NULL CHECK (kind IN ('conn','file','alarm')),
  peer_id TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL,
  received_at INTEGER NOT NULL
);
```

`db::connect` executa `PRAGMA foreign_keys = ON` e `journal_mode = WAL` (via `SqliteConnectOptions`).

---

### Task 1: Esqueleto do painel, banco e harness de testes

**Files:**
- Create: `.gitignore` (inclui `painel/target/`, `painel/dados/`, `servidor/.env`, `servidor/dados/`), `painel/Cargo.toml`, `painel/migrations/0001_inicial.sql`, `painel/src/{main,lib,config,db,state,error,clock}.rs`, `painel/tests/common/mod.rs`, `painel/tests/health.rs`

**Interfaces:**
- Produces:
  - `pub struct Config { pub db_url: String, pub bind: String, pub public_url: String, pub cookie_secure: bool }` e `Config::from_env() -> anyhow::Result<Config>`
  - `pub async fn db::connect(url: &str) -> anyhow::Result<SqlitePool>` (cria arquivo/diretório se faltar; roda `sqlx::migrate!("./migrations")`)
  - `#[derive(Clone)] pub struct AppState { pub pool: SqlitePool, pub config: Arc<Config>, pub limiter: Arc<auth::limiter::LoginLimiter> }` (o tipo `LoginLimiter` nasce na Task 2; nesta task crie o módulo `auth` com o struct vazio e `new()`)
  - `pub fn build_router(state: AppState) -> axum::Router` (por enquanto só `GET /saude` → `200 "ok"`)
  - `pub struct ApiError { pub status: StatusCode, pub message: String }` com `IntoResponse` → JSON `{"error": message}`; construtores `ApiError::bad_request(msg)`, `unauthorized()` (mensagem "Sessão expirada, faça login novamente"), `forbidden(msg)`, `not_found(msg)`, `internal(err)` (loga o erro, responde "Erro interno")
  - `clock::now() -> i64`; `clock::format_ts(ts: i64) -> String` (UTC−03:00, `dd/mm/aaaa HH:MM`)
  - `tests/common/mod.rs`: `pub struct TestApp { pub router: Router, pub pool: SqlitePool }`, `pub async fn test_app() -> TestApp` (pool `sqlite::memory:` com `max_connections(1)`, config com `public_url = "http://localhost"` e `cookie_secure = false`), `pub async fn send(app: &TestApp, req: Request<Body>) -> (StatusCode, HeaderMap, String)`, `pub fn json_req(method: &str, uri: &str, body: serde_json::Value, bearer: Option<&str>) -> Request<Body>`

- [ ] **Step 1:** `git init` na raiz `pcep-desk/` e criar `.gitignore`. `cargo new painel --lib` e adicionar binário em `src/main.rs`. Adicionar dependências com `cargo add` (versões do Tech Stack; `sqlx` com features `sqlite,runtime-tokio,migrate`, `askama_web` com feature `axum-0.8`, `tokio` com `full`, `tower` com `util` em dev-dependencies, `http-body-util` em dev-dependencies, `anyhow`, `tracing`, `tracing-subscriber`).
- [ ] **Step 2: Testes que falham** em `tests/health.rs`:
  - `health_ok`: `GET /saude` → 200, corpo `"ok"`.
  - `migrations_create_tables`: após `test_app()`, `SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('users','tokens','user_devices','devices','clients','computers','ab_personal','ab_peers','ab_tags','connections','audit_raw')` = 11.
  - `format_ts_brt`: `clock::format_ts(1762257600)` = `"04/11/2025 09:00"` (12:00 UTC).
- [ ] **Step 3:** `cargo test --test health` → FAIL (não compila/rotas inexistentes).
- [ ] **Step 4:** Implementar as interfaces acima e a migration com o esquema completo. `main.rs`: `serve` lê `Config`, conecta, sobe `axum::serve` em `config.bind`, logs com `tracing_subscriber` (nível via `RUST_LOG`, padrão `info`).
- [ ] **Step 5:** `cargo test --test health` → 3 passed. `cargo clippy --all-targets -- -D warnings` limpo.
- [ ] **Step 6: Commit** (com autorização): `feat(painel): esqueleto, banco e harness de testes`

### Task 2: Usuários, senhas, tokens e criação do administrador

**Files:**
- Create: `painel/src/auth/{password,tokens,limiter}.rs`, `painel/src/users.rs`, `painel/tests/auth_core.rs`
- Modify: `painel/src/main.rs` (subcomando `criar-admin`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, sqlx::FromRow)] pub struct User { pub id: i64, pub username: String, pub display_name: String, pub email: String, pub is_admin: bool, pub active: bool }`
  - `users::create(pool, username: &str, display_name: &str, password: &str, is_admin: bool) -> Result<i64, UserError>` — `UserError::{UsernameTaken, Invalid(String), Db(sqlx::Error)}`; valida username `^[a-z0-9._-]{3,32}$` (minúsculo após `to_lowercase`), display_name 2–60 caracteres após `trim`, senha ≥ 8 caracteres.
  - `users::find_by_username(pool, &str) -> sqlx::Result<Option<(User, String /*password_hash*/)>>`, `users::get(pool, id) -> sqlx::Result<Option<User>>`, `users::list(pool) -> sqlx::Result<Vec<User>>`, `users::update_profile(pool, id, display_name: &str, email: &str, is_admin: bool)`, `users::set_password(pool, id, &str)`, `users::set_active(pool, id, active: bool)` — `set_active(false)` também chama `revoke_user_tokens`.
  - `password::hash_password(&str) -> String`, `password::verify_password(password: &str, hash: &str) -> bool`
  - `tokens::issue_token(pool, user_id: i64, kind: TokenKind) -> sqlx::Result<String>` (retorna o token cru); `tokens::resolve_token(pool, raw: &str, kind: TokenKind) -> sqlx::Result<Option<User>>` (None se inexistente, expirado, de outro `kind` ou usuário inativo); `tokens::revoke_token(pool, raw: &str)`; `tokens::revoke_user_tokens(pool, user_id)`. `pub enum TokenKind { App, Web }` (180 dias / 12 horas).
  - `LoginLimiter::new()`, `check(&self, key: &str, now: i64) -> bool` (false = bloqueado), `record_failure(&self, key: &str, now: i64)`, `reset(&self, key: &str)`. Regra: 5 falhas dentro de 900 s bloqueiam até 900 s após a primeira falha da janela. Chave = username em minúsculas. `Mutex<HashMap<..>>`.
  - CLI: `painel criar-admin --usuario <u> --nome "<nome>"`, senha lida de `PAINEL_SENHA_ADMIN` ou do stdin (sem eco não é obrigatório). Falha com mensagem clara se o usuário já existe.

- [ ] **Step 1: Testes que falham** (`tests/auth_core.rs`):
  - `password_roundtrip`: `verify_password("segredo123", &hash_password("segredo123"))` true; com `"errado"` false.
  - `create_user_validates`: username `"Jo"` → `Invalid`; senha `"curta"` → `Invalid`; `"maria"` duas vezes → segunda `UsernameTaken`; `"MARIA"` após `"maria"` → `UsernameTaken`.
  - `token_resolves_and_revokes`: `issue_token(App)` → `resolve_token(App)` retorna o usuário; `resolve_token(Web)` com o mesmo token → None; após `revoke_user_tokens` → None.
  - `token_expired`: atualizar `expires_at` para `now()-1` via SQL → None.
  - `inactive_user_token_none`: `set_active(false)` → `resolve_token` None e tabela `tokens` sem linhas do usuário.
  - `limiter_blocks_after_five`: 5× `record_failure("maria", 1000)` → `check("maria", 1001)` false; `check("maria", 1000+901)` true; `reset` libera.
- [ ] **Step 2:** `cargo test --test auth_core` → FAIL.
- [ ] **Step 3:** Implementar. Tokens: `rand` para 32 bytes, `base64::URL_SAFE_NO_PAD`, `sha2::Sha256` → hex.
- [ ] **Step 4:** `cargo test --test auth_core` → 6 passed; `cargo clippy --all-targets -- -D warnings` limpo.
- [ ] **Step 5:** Verificar o CLI: `PAINEL_URL_PUBLICA=http://localhost PAINEL_DB=sqlite:///tmp/pcep-teste.db PAINEL_SENHA_ADMIN=senha12345 cargo run -- criar-admin --usuario admin --nome "Administrador"` → mensagem "Administrador criado"; rodar de novo → erro "usuário já existe", código de saída ≠ 0.
- [ ] **Step 6: Commit** (com autorização): `feat(painel): usuários, senhas, tokens e criar-admin`

### Task 3: API de conta e grupos (login, currentUser, logout, login-options, users/peers/device-group)

**Files:**
- Create: `painel/src/api/{mod,extract,account,groups}.rs`, `painel/tests/account.rs`, `painel/tests/groups.rs`
- Modify: `painel/src/lib.rs` (monta `/api` com `DefaultBodyLimit::max(65536)`)

**Interfaces:**
- Consumes: Task 2 (`users::*`, `tokens::*`, `LoginLimiter`).
- Produces:
  - `pub struct ApiUser(pub User)` — extractor: lê `Authorization: Bearer <t>`, `resolve_token(App)`; falha → `ApiError::unauthorized()` (401).
  - `fn user_payload(u: &User) -> serde_json::Value` = `{"name": username, "display_name", "email", "note": "", "status": 1, "is_admin"}` (usado por login, currentUser e `/api/users`).
  - `pub fn api::router() -> Router<AppState>`

**Comportamento (contrato do app):**
- `GET /api/login-options` → `200 []`.
- `POST /api/login` corpo `{username, password, id?, uuid?, type?, deviceInfo?, autoLogin?}`. Aceita só `type` ausente ou `"account"` (outros → 200 `{"error":"Método de login não suportado"}`). Bloqueado pelo limiter → 200 `{"error":"Muitas tentativas. Aguarde 15 minutos."}`. Usuário/senha errados → 200 `{"error":"Usuário ou senha incorretos"}` + `record_failure`. Inativo → 200 `{"error":"Usuário desativado"}`. Sucesso → `reset`, `issue_token(App)`; se `id` não vazio, upsert em `user_devices(peer_id=id, user_id, uuid, last_login_at)`; resposta `{"type":"access_token","access_token":<t>,"user":user_payload}`.
- `POST /api/currentUser` (ApiUser) corpo `{id, uuid}` → upsert `user_devices` se `id` não vazio; resposta `user_payload`. Token inválido → **401**.
- `POST /api/logout` (Bearer opcional) → revoga o token se presente; `200` corpo vazio.
- `GET /api/users?current&pageSize` (ApiUser) → paginado com `user_payload` de usuários ativos ordenados por `display_name`.
- `GET /api/peers?...` e `GET /api/device-group/accessible?...` (ApiUser) → `{"total":0,"data":[]}`.

- [ ] **Step 1: Testes que falham** (`tests/account.rs`, helpers de criação de usuário via `users::create`):
  - `login_success_returns_token_and_user`: `type == "access_token"`, `user.display_name == "Maria – PCEP"`, `user.is_admin == false`; com `id:"123456789"` grava `user_devices`.
  - `login_wrong_password_error`: corpo `{"error":"Usuário ou senha incorretos"}`.
  - `login_case_insensitive_username`: `"MARIA"` loga.
  - `login_lockout_after_five_failures`: 5 senhas erradas, 6ª com a senha certa → `"Muitas tentativas. Aguarde 15 minutos."`.
  - `current_user_ok_and_401`: Bearer válido → `name == "maria"`; sem Bearer → 401; Bearer lixo → 401.
  - `disabled_user_token_rejected`: login, `set_active(false)`, `currentUser` → 401; novo login → `"Usuário desativado"`.
  - `logout_revokes`: logout, depois `currentUser` → 401.
  - `login_options_empty_array`: corpo `[]`.
  - (`tests/groups.rs`) `users_paginated`: 3 usuários ativos + 1 inativo → `total == 3`; `pageSize=2,current=2` → 1 item. `peers_and_groups_empty`: `{"total":0,"data":[]}`; sem Bearer → 401.
- [ ] **Step 2:** `cargo test --test account --test groups` → FAIL.
- [ ] **Step 3:** Implementar `extract.rs`, `account.rs`, `groups.rs` e montar o router.
- [ ] **Step 4:** `cargo test --test account --test groups` → todos passam; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(api): login, sessão do app e grupos`

### Task 4: API de dispositivos (heartbeat, sysinfo, sysinfo_ver)

**Files:**
- Create: `painel/src/devices.rs`, `painel/src/api/devices.rs`, `painel/tests/devices.rs`

**Interfaces:**
- Produces:
  - `devices::touch(pool, peer_id: &str, uuid: &str, version: &str, now: i64)` (upsert; atualiza `last_seen_at`)
  - `devices::upsert_sysinfo(pool, peer_id, uuid, info: &SysInfo, now: i64)`; `pub struct SysInfo { hostname, os, username, cpu, memory, version: String }` (campos ausentes = `""`)
  - `devices::is_online(last_seen_at: i64, now: i64) -> bool` = `now - last_seen_at <= 60`
  - `devices::get(pool, peer_id) -> sqlx::Result<Option<Device>>`; `#[derive(FromRow)] pub struct Device { peer_id, uuid, version, hostname, os, username, cpu, memory: String, first_seen_at, last_seen_at: i64 }`
  - `devices::list_unlinked(pool) -> sqlx::Result<Vec<Device>>` — dispositivos cujo `peer_id` não está em `computers` nem em `user_devices`, ordem `last_seen_at DESC`.

**Comportamento:**
- `POST /api/heartbeat` corpo `{id, uuid, ver, conns?, modified_at?}`; `id` vazio/ausente ou JSON inválido → 400 `{"error":"Requisição inválida"}`. Sucesso → `touch` e resposta `{}` (JSON).
- `POST /api/sysinfo` corpo com `id, uuid, hostname, os, username, cpu, memory, version` (todos string; demais chaves ignoradas — **não gravar `preset-address-book-password`**). Sucesso → texto `SYSINFO_UPDATED` (`content-type: text/plain`). `id` vazio → texto `ID_NOT_FOUND`.
- `POST /api/sysinfo_ver` → texto vazio, 200.
- Sem autenticação nestas rotas (o app não envia token).

- [ ] **Step 1: Testes que falham** (`tests/devices.rs`):
  - `heartbeat_creates_and_touches_device`: heartbeat `{"id":"111222333","uuid":"dXU=","ver":"1004005"}` → 200 `{}`; `devices::get` existe com `version == "1004005"`.
  - `sysinfo_updates_fields`: sysinfo com `hostname:"PC-CAIXA"`, `os:"Windows / 11 Pro"`, `preset-address-book-password:"x"` → corpo `SYSINFO_UPDATED`; `hostname == "PC-CAIXA"`; nenhuma coluna/linha contém `"x"` (consulta `SELECT * FROM devices`).
  - `sysinfo_without_id`: corpo `ID_NOT_FOUND`.
  - `heartbeat_rejects_garbage_and_oversize`: corpo `"não é json"` → 400; corpo JSON com 70 000 bytes → 413; `SELECT count(*) FROM devices` = 0.
  - `unlinked_excludes_known`: 3 dispositivos; um em `computers` (inserção SQL direta com cliente), um em `user_devices` → `list_unlinked` retorna só o terceiro.
  - `is_online_window`: `is_online(1000, 1060)` true, `(1000, 1061)` false.
- [ ] **Step 2:** `cargo test --test devices` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** `cargo test --test devices` → passa; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(api): heartbeat e sysinfo`

### Task 5: API de histórico (audit conn/file/alarm)

**Files:**
- Create: `painel/src/connections.rs`, `painel/src/api/audit.rs`, `painel/tests/audit.rs`

**Interfaces:**
- Produces:
  - `connections::apply_conn_event(pool, ev: &ConnEvent, now: i64) -> sqlx::Result<()>`; `pub struct ConnEvent { pub peer_id: String, pub conn_id: i64, pub session_id: String, pub kind: ConnEventKind }`; `pub enum ConnEventKind { New { ip: String }, Authorized { operator_peer_id: String, operator_name: String, conn_type: Option<i64> }, Close }`
  - `connections::query(pool, f: &HistoryFilter) -> sqlx::Result<Vec<HistoryRow>>` — `pub struct HistoryFilter { pub user_id: Option<i64>, pub client_id: Option<i64>, pub from: Option<i64>, pub to: Option<i64>, pub limit: i64 }`; `pub struct HistoryRow { pub id: i64, pub started_at: i64, pub authorized_at: Option<i64>, pub ended_at: Option<i64>, pub peer_id: String, pub computer_label: Option<String>, pub client_id: Option<i64>, pub client_name: Option<String>, pub operator_name: String, pub user_display_name: Option<String>, pub ip: String, pub conn_type: Option<i64> }` (LEFT JOIN `computers`, `clients`, `users`; ordem `started_at DESC`).
  - `pub fn status_label(r: &HistoryRow) -> &'static str`: `authorized_at` presente → `"Conectado"`; ausente e `ended_at` presente → `"Não concluída"`; ambos ausentes → `"Aguardando aceite"`.
  - `pub fn type_label(t: Option<i64>) -> &'static str`: 0 "Acesso remoto", 1 "Transferência de arquivos", 2 "Redirecionamento de porta", 3 "Câmera", 4 "Terminal", outro/None "—".

**Comportamento:**
- `POST /api/audit/conn`: corpo JSON com `id` (string), `conn_id` (inteiro), `session_id` (número ou string — guardar `to_string()` do `Value`), `nonce` (string, opcional) e:
  - `action == "new"` + `ip` → `New`;
  - `action == "close"` → `Close`;
  - sem `action` e com `peer` = `[my_id, my_name]` → `Authorized` (`type` inteiro opcional).
  - Outros formatos → grava em `audit_raw` e responde 200 vazio (não quebrar o app com versões futuras).
- Dedup: inserir em `audit_raw(nonce, kind, peer_id, body, received_at)` primeiro; se o `nonce` já existe (conflito UNIQUE), responder **200 vazio** sem reaplicar. Sem `nonce` → sempre aplica.
- `apply_conn_event`: localizar o registro aberto mais recente com mesmo `peer_id` e `conn_id` e `ended_at IS NULL`. `New` → sempre insere novo registro (`started_at = now`). `Authorized` → atualiza o aberto (`authorized_at = now`, `operator_*`, `conn_type`, `user_id` = `user_devices.user_id` do `operator_peer_id`, se houver); se não houver aberto, insere já autorizado com `started_at = now`. `Close` → `ended_at = now` no aberto; sem aberto, ignora.
- `POST /api/audit/file` e `/api/audit/alarm`: só gravam em `audit_raw` (com dedup por `nonce`); 200 vazio.
- JSON inválido → 400 `{"error":"Requisição inválida"}`, nada gravado.
- Sucesso é **sempre corpo vazio** (o app trata corpo não vazio como falha e reenvia).

- [ ] **Step 1: Testes que falham** (`tests/audit.rs`):
  - `full_lifecycle`: new (`ip:"200.1.2.3"`) → authorized (`peer:["987654321","Maria – PCEP"]`, `type:0`) → close, todos com `id:"111222333"`, `conn_id:7`. Uma linha em `connections` com `authorized_at`, `ended_at` preenchidos, `operator_name == "Maria – PCEP"`; corpo de cada resposta `""` e status 200.
  - `user_linked_by_operator_device`: com `user_devices("987654321" → maria)`, `user_id` = id da Maria.
  - `duplicate_nonce_ignored`: mesmo evento `new` com `nonce:"abc"` enviado 2× → 1 linha em `connections`, 1 em `audit_raw`, ambas respostas 200 vazias.
  - `authorized_before_new_creates_single_record`: authorized sem new prévio → 1 linha com `authorized_at` preenchido; close depois → `ended_at` preenchido, ainda 1 linha.
  - `refused_attempt_status`: new + close → `status_label == "Não concluída"`.
  - `session_id_large_number`: `session_id: 18446744073709551615` → aceito, gravado `"18446744073709551615"`.
  - `audit_rejects_garbage`: corpo `"{"` → 400; `audit_raw` vazio.
  - `file_and_alarm_stored_raw`: 1 POST em cada → 2 linhas em `audit_raw` com `kind` correto.
  - `query_filters`: 2 conexões de usuários diferentes → filtro `user_id` retorna só uma; filtro `client_id` funciona via `computers`.
- [ ] **Step 2:** `cargo test --test audit` → FAIL.
- [ ] **Step 3:** Implementar. Parse via `serde_json::Value` (não `struct` rígido) para tolerar campos novos.
- [ ] **Step 4:** `cargo test --test audit` → passa; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(api): histórico de conexões via audit`

### Task 6: Clientes e computadores (domínio)

**Files:**
- Create: `painel/src/clients.rs`, `painel/tests/clients.rs`

**Interfaces:**
- Produces:
  - `pub fn normalize_peer_id(raw: &str) -> Option<String>` — remove espaços, pontos e hífens; aceita só dígitos com 6–12 caracteres **ou** id alfanumérico `^[A-Za-z0-9_-]{6,16}$` (ids personalizados do RustDesk); senão None.
  - `#[derive(FromRow)] pub struct Client { id: i64, name: String, document: String, note: String }`, `pub struct Computer { id: i64, client_id: i64, peer_id: String, label: String, note: String }`
  - `clients::create(pool, name, document, note) -> Result<i64, ClientError>`; `update(pool, id, name, document, note)`; `delete(pool, id)`; `get(pool, id)`; `search(pool, q: &str, limit: i64) -> Vec<Client>` (busca em `clients.name`, `clients.document` e em `computers.label`/`computers.peer_id` dos computadores, `LIKE %q%`, sem diferenciar maiúsculas; q vazio → todos por nome).
  - `clients::add_computer(pool, client_id, peer_id_raw, label, note) -> Result<i64, ClientError>`; `update_computer(pool, id, label, note)`; `remove_computer(pool, id)`; `list_computers(pool, client_id) -> Vec<Computer>`
  - `pub enum ClientError { NameTaken, InvalidPeerId, PeerIdTaken { client_name: String }, Invalid(String), Db(sqlx::Error) }` com `Display` em português: `"Já existe um cliente com esse nome"`, `"Código de conexão inválido"`, `"Esse código já está cadastrado no cliente {client_name}"`.
  - `clients::all_computers_with_client(pool) -> Vec<(Computer, Client)>` (para o catálogo compartilhado).

- [ ] **Step 1: Testes que falham** (`tests/clients.rs`):
  - `peer_id_normalized`: `"123 456 789"` → `Some("123456789")`; `"1-2.3"` → None; `"abc"` → None; `"pcep_caixa1"` → Some.
  - `duplicate_peer_id_rejected`: mesmo código em dois clientes → `PeerIdTaken { client_name: "Padaria X" }`.
  - `search_matches_computer_label_and_id`: cliente "Padaria X" com computador "Caixa" id `123456789` → busca `"caixa"`, `"4567"` e `"padaria"` encontram o cliente.
  - `client_name_unique_case_insensitive`: `"padaria x"` após `"Padaria X"` → `NameTaken`.
  - `delete_client_cascades_computers`.
- [ ] **Step 2:** `cargo test --test clients` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** `cargo test --test clients` → passa; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(painel): clientes e computadores`

### Task 7: API do catálogo de endereços (modo novo)

**Files:**
- Create: `painel/src/addressbook.rs`, `painel/src/api/ab.rs`, `painel/tests/ab.rs`

**Interfaces:**
- Consumes: Task 3 (`ApiUser`), Task 4 (`devices::get` para hostname/plataforma), Task 6 (`all_computers_with_client`).
- Produces:
  - `pub const SHARED_AB_GUID: &str = "clientes-pcep"`, `pub const SHARED_AB_NAME: &str = "Clientes PCEP"`
  - `addressbook::personal_guid(pool, user_id) -> sqlx::Result<String>` (cria `uuid::Uuid::new_v4()` na primeira chamada)
  - `addressbook::shared_peers(pool) -> sqlx::Result<Vec<serde_json::Value>>` — por computador: `{"id": peer_id, "alias": "{cliente} – {label}", "tags": [cliente], "hostname": devices.hostname, "username": devices.username, "platform": platform_of(devices.os), "note": computer.note}`
  - `pub fn platform_of(os: &str) -> &'static str`: prefixo antes de `" / "` em minúsculas → `"windows"`→`"Windows"`, `"linux"`→`"Linux"`, `"macos"`→`"Mac OS"`, `"android"`→`"Android"`; contém `"linux"` → `"Linux"`; senão `""`.
  - `pub fn tag_color(name: &str) -> i64` = `0xFF000000 | (fnv1a32(name) & 0x00FFFFFF)` (determinístico).

**Comportamento (todas com `ApiUser`; sem token → 401):**
- `POST /api/ab/settings` → `{"max_peer_one_ab": 0}`.
- `POST /api/ab/personal` → `{"guid": personal_guid}`.
- `POST /api/ab/shared/profiles?current&pageSize` → `{"total":1,"data":[{"guid":"clientes-pcep","name":"Clientes PCEP","owner":"PCEP","note":"","rule":1}]}` (paginação respeitada: `current=2` → `data: []`).
- `POST /api/ab/peers?ab=<guid>&current&pageSize` → paginado. Shared → `shared_peers`; pessoal (só do dono) → linhas de `ab_peers` como `{"id","alias","tags"(array),"username","hostname","platform","hash","note"}`. Guid de outro usuário ou inexistente → 404 `{"error":"Catálogo não encontrado"}`.
- `POST /api/ab/tags/{guid}` → **lista** JSON `[{"name","color"}]`. Shared → nomes de clientes distintos com `tag_color`; pessoal → `ab_tags`.
- Escrita (somente no pessoal do dono; no shared → 403 `{"error":"Catálogo somente leitura"}`), sucesso = 200 corpo vazio:
  - `POST /api/ab/peer/add/{guid}` corpo = objeto peer (`id` obrigatório; `alias`, `tags`, `username`, `hostname`, `platform`, `hash`, `note` opcionais; outros campos ignorados). Id existente → atualiza.
  - `PUT /api/ab/peer/update/{guid}` corpo `{"id", ...campos}` → atualiza só os campos presentes (`tags`, `alias`, `note`, `hash`, `username`, `hostname`, `platform`); `password` ignorado. Id inexistente → 404.
  - `DELETE /api/ab/peer/{guid}` corpo array de ids.
  - `POST /api/ab/tag/add/{guid}` `{"name","color"}`; `PUT /api/ab/tag/rename/{guid}` `{"old","new"}` (também renomeia a tag dentro de `ab_peers.tags`); `PUT /api/ab/tag/update/{guid}` `{"name","color"}`; `DELETE /api/ab/tag/{guid}` array de nomes (remove também das listas de `ab_peers.tags`).

- [ ] **Step 1: Testes que falham** (`tests/ab.rs`):
  - `personal_guid_stable`: duas chamadas a `/api/ab/personal` → mesmo `guid`.
  - `shared_profile_listed_read_only`: `data[0].rule == 1`, `name == "Clientes PCEP"`.
  - `shared_peers_from_clients`: cliente "Padaria X" + computador "Caixa" `123456789` + dispositivo com `os:"Windows / 11"` → peer `alias == "Padaria X – Caixa"`, `tags == ["Padaria X"]`, `platform == "Windows"`; `total == 1`.
  - `shared_tags_list`: resposta é array com `{"name":"Padaria X","color": tag_color("Padaria X")}`.
  - `shared_write_forbidden`: `peer/add/clientes-pcep` → 403 com `error`.
  - `personal_crud`: add `{id:"555666777",alias:"Meu PC",tags:["casa"]}` → peers lista; update `{id, alias:"Casa"}` → alias mudou e tags preservadas; delete `["555666777"]` → vazio. Respostas de escrita: 200, corpo `""`.
  - `tag_rename_updates_peers`: tag "casa" em peer → rename para "lar" → peer `tags == ["lar"]`.
  - `other_users_personal_404`: Maria lê guid pessoal do João → 404.
  - `ab_requires_token`: sem Bearer → 401.
- [ ] **Step 2:** `cargo test --test ab` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** `cargo test --test ab` → passa; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(api): catálogo de endereços pessoal e compartilhado`

### Task 8: Painel web — sessão, layout e colaboradores

**Files:**
- Create: `painel/src/web/{mod,session,login,users}.rs`, `painel/templates/{base,entrar,colaboradores,colaborador_form}.html`, `painel/static/{htmx.min.js,painel.css}`, `painel/tests/web_auth.rs`, `painel/tests/web_users.rs`
- Modify: `painel/src/lib.rs` (monta rotas web e `ServeDir` em `/static`)

**Interfaces:**
- Consumes: Task 2.
- Produces:
  - `pub struct WebUser(pub User)` — extractor: cookie `pcep_sessao` → `resolve_token(Web)`; ausente/inválido → redirect `303` para `/entrar`.
  - `pub struct AdminUser(pub User)` — como `WebUser` + `is_admin`; senão `403` com página "Acesso restrito ao administrador".
  - Middleware `require_same_origin`: em métodos ≠ GET/HEAD fora de `/api`, se o header `Origin` existir e for diferente da origem de `config.public_url` → 403. (Cookie também tem `SameSite=Strict`.)
  - Template base com menu: Clientes, Histórico, Não vinculados, Colaboradores (só admin), Sair; mostra `display_name`.

**Comportamento:**
- `GET /entrar` formulário; `POST /entrar` (form `usuario`, `senha`) usa o mesmo `LoginLimiter` e mensagens da API; sucesso → cookie `pcep_sessao` (`HttpOnly`, `SameSite=Strict`, `Path=/`, `Secure` se `cookie_secure`, `Max-Age=43200`) e redirect para `/clientes`. Erro → reexibe formulário com a mensagem (status 200).
- `POST /sair` revoga o token web e apaga o cookie.
- `GET /` → redirect `/clientes`.
- `/colaboradores` (AdminUser): tabela com nome de exibição, usuário, papel, situação; botões Novo, Editar, Redefinir senha, Desativar/Reativar. Formulário de criação: usuário, nome de exibição (ajuda: "É o nome que o cliente vê ao aceitar a conexão, ex.: Maria – PCEP"), e-mail, senha, administrador (checkbox). Erros de `UserError` exibidos no formulário.
- Desativar → `users::set_active(false)` (revoga app e web). O admin não pode desativar a si mesmo nem remover o próprio papel de admin (mensagem "Você não pode desativar a própria conta").
- htmx baixado de `https://unpkg.com/htmx.org@2/dist/htmx.min.js` e versionado em `static/` (sem CDN em produção). CSS próprio simples e legível, sem framework.

- [ ] **Step 1: Testes que falham**:
  - (`web_auth.rs`) `pages_require_login`: `GET /clientes` sem cookie → 303 `Location: /entrar`.
  - `web_login_sets_cookie`: POST `/entrar` válido → 303 `/clientes`, `Set-Cookie` com `pcep_sessao`, `HttpOnly`, `SameSite=Strict`.
  - `web_login_wrong_password_shows_error`: 200 e HTML contém "Usuário ou senha incorretos".
  - `app_token_not_valid_as_web_cookie`: cookie com token `App` → 303 `/entrar`.
  - `cross_origin_post_forbidden`: POST `/sair` com `Origin: https://malicioso.example` → 403.
  - (`web_users.rs`) `collaborator_cannot_manage_users`: colaborador em `/colaboradores` → 403.
  - `admin_creates_collaborator`: POST de criação → listado com "Maria – PCEP".
  - `disable_revokes_web_and_app`: Maria com token app e cookie web; admin desativa → `currentUser` 401 e `/clientes` com cookie dela → 303.
  - `admin_cannot_disable_self`: mensagem exibida, continua ativo.
- [ ] **Step 2:** `cargo test --test web_auth --test web_users` → FAIL.
- [ ] **Step 3:** Implementar (Askama: `#[derive(Template, WebTemplate)]`; confirme a API exata do askama_web 0.16 na documentação do crate).
- [ ] **Step 4:** Testes passam; clippy limpo.
- [ ] **Step 5: Verificação manual:** `PAINEL_URL_PUBLICA=http://localhost:8080 PAINEL_COOKIE_SEGURO=false cargo run`, criar admin pelo CLI, entrar no navegador, criar colaborador, desativar, reativar.
- [ ] **Step 6: Commit** (com autorização): `feat(web): login, layout e colaboradores`

### Task 9: Painel web — clientes, computadores e não vinculados

**Files:**
- Create: `painel/src/web/clients.rs`, `painel/templates/{clientes,clientes_lista,cliente,cliente_form,computador_form,nao_vinculados}.html`, `painel/tests/web_clients.rs`

**Interfaces:**
- Consumes: Task 4 (`devices::get`, `is_online`, `list_unlinked`), Task 5 (`connections::query` com `client_id`), Task 6 (`clients::*`).

**Comportamento (WebUser — todos os colaboradores podem cadastrar e editar; excluir cliente só AdminUser):**
- `/clientes`: campo de busca com `hx-get="/clientes/lista" hx-trigger="input changed delay:300ms"` que troca só a tabela (`clientes_lista.html`). Cada linha: nome, documento, nº de computadores, quantos online.
- `/clientes/{id}`: dados do cliente; tabela de computadores com rótulo, **código de conexão em destaque com botão copiar** (JS mínimo `navigator.clipboard`), status Online/Offline (bolinha + texto, não só cor), sistema, nome do computador e usuário (de `devices`), "visto por último"; e as últimas 20 conexões do cliente.
- Formulários: novo/editar cliente; adicionar/editar/remover computador (código normalizado com `normalize_peer_id`; erros de `ClientError` exibidos no formulário).
- `/nao-vinculados`: `list_unlinked` com código, nome do computador, sistema, visto por último; ação "Vincular" abre o formulário de computador com o código preenchido e seleção do cliente (select com busca simples).

- [ ] **Step 1: Testes que falham** (`tests/web_clients.rs`):
  - `search_fragment_filters`: dois clientes; `GET /clientes/lista?q=pada` contém "Padaria X" e não contém o outro.
  - `add_computer_normalizes_and_shows`: POST com `"123 456 789"` → página do cliente contém `"123456789"`.
  - `duplicate_peer_id_message`: HTML contém "Esse código já está cadastrado no cliente Padaria X".
  - `online_status_shown`: dispositivo com `last_seen_at = now` → página contém "Online"; com `now-120` → "Offline".
  - `unlinked_page_and_link`: dispositivo desconhecido aparece em `/nao-vinculados`; após vincular, some da lista.
  - `collaborator_cannot_delete_client`: 403.
- [ ] **Step 2:** `cargo test --test web_clients` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** Testes passam; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(web): clientes, computadores e não vinculados`

### Task 10: Painel web — histórico

**Files:**
- Create: `painel/src/web/history.rs`, `painel/templates/historico.html`, `painel/tests/web_history.rs`

**Interfaces:**
- Consumes: Task 5 (`connections::query`, `status_label`, `type_label`), `clock::format_ts`.

**Comportamento:**
- `/historico` (WebUser): filtros por colaborador (só admin vê o select; colaborador vê **só as próprias** conexões, `user_id` forçado), cliente (select), período (`de`/`ate` como datas `aaaa-mm-dd` interpretadas em UTC−03:00, `ate` inclusivo até 23:59:59). Limite 500 linhas, aviso "mostrando as 500 mais recentes" quando atingir.
- Colunas: início, colaborador (`user_display_name` ou `operator_name` com o rótulo "(não vinculado)"), cliente – computador (ou o código quando não cadastrado), tipo, situação (`status_label`), duração (`ended_at - authorized_at` em `HH:MM:SS`; vazio se não concluída), IP.

- [ ] **Step 1: Testes que falham** (`tests/web_history.rs`):
  - `collaborator_sees_only_own`: conexões de Maria e João; Maria vê a dela e não a do João.
  - `admin_filters_by_user_and_client`.
  - `period_filter_inclusive_end`: conexão às 23:30 BRT do dia `ate` aparece.
  - `duration_and_status_render`: conexão autorizada às T e encerrada às T+3725 → "01:02:05" e "Conectado"; new+close → "Não concluída".
- [ ] **Step 2:** `cargo test --test web_history` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** `cargo test` (suíte inteira) → tudo passa; clippy limpo.
- [ ] **Step 5: Commit** (com autorização): `feat(web): histórico de conexões`

### Task 11: Implantação (Docker Compose, Caddy, backup, README)

**Files:**
- Create: `painel/Dockerfile`, `painel/.dockerignore`, `servidor/docker-compose.yml`, `servidor/Caddyfile`, `servidor/.env.example`, `servidor/backup.sh`, `servidor/README.md`

**Decisões:**
- `painel/Dockerfile`: multi-stage (`rust:1.95-slim` build `--release` → `debian:stable-slim` com `ca-certificates`), usuário não-root, `EXPOSE 8080`, volume `/dados`, `PAINEL_DB=sqlite:///dados/painel.db`.
- `docker-compose.yml`:
  - `hbbs`: imagem `rustdesk/rustdesk-server:1.1.16`, `command: hbbs -k _`, `network_mode: host`, volume `./dados/rustdesk:/root`, `restart: unless-stopped`, `depends_on: [hbbr]`.
  - `hbbr`: mesma imagem, `command: hbbr -k _`, `network_mode: host`, mesmo volume.
  - `painel`: build `../painel`, `env_file: .env`, volume `./dados/painel:/dados`, porta só em `127.0.0.1:8080:8080`.
  - `caddy`: `caddy:2`, portas `80` e `443`, `Caddyfile` montado, volumes de dados/config do Caddy.
- `Caddyfile`: `{$PAINEL_DOMINIO} { reverse_proxy host.docker.internal:8080 }` (com `extra_hosts: ["host.docker.internal:host-gateway"]` no serviço caddy).
- `.env.example`: `PAINEL_DOMINIO=desk.pcepcontabil.com.br`, `PAINEL_URL_PUBLICA=https://desk.pcepcontabil.com.br`, `PAINEL_COOKIE_SEGURO=true`, `RUST_LOG=info`.
- `backup.sh`: `sqlite3 .backup` não está disponível na imagem; usar `docker compose exec painel` com o comando `VACUUM INTO` via subcomando do painel → adicionar subcomando `painel backup <destino>` (executa `VACUUM INTO ?`) em `main.rs`, com teste `backup_creates_file` em `tests/health.rs`. Script copia também `dados/rustdesk/id_ed25519*` e mantém 14 dias.
- `README.md` (português, para o usuário): requisitos; portas a liberar (TCP 21115–21119, UDP 21116, TCP 80/443); DNS (registro A, ou DDNS+CNAME no cenário servidor no escritório); `docker compose up -d`; criar admin (`docker compose exec painel painel criar-admin ...`); onde achar a chave pública (`dados/rustdesk/id_ed25519.pub`); **como configurar o app RustDesk original para testar** (Configurações → Rede → Servidor de ID/Relay: domínio; Servidor de API: `https://<domínio>`; Chave: conteúdo do `.pub`); backup e restauração.

- [ ] **Step 1:** Teste `backup_creates_file` (subcomando gera arquivo SQLite válido com a tabela `users`) → FAIL → implementar → PASS.
- [ ] **Step 2:** `cargo build --release` sem warnings.
- [ ] **Step 3:** Se Docker estiver disponível: `docker compose -f servidor/docker-compose.yml config` sem erro e `docker build painel` conclui. Se não estiver (situação atual da máquina de desenvolvimento), registrar como **pendente de verificação no servidor** no relatório final — não marcar como verificado.
- [ ] **Step 4: Commit** (com autorização): `feat(servidor): docker compose, caddy, backup e README`

### Task 12: Validação ponta a ponta com o RustDesk original

Sem código novo, exceto correções que surgirem (cada correção com teste que reproduz o problema).

- [ ] **Step 1:** Subir localmente: `hbbs`/`hbbr` 1.1.16 (binários do release do `rustdesk-server` ou Docker) com `-k _`, e o painel em `http://<ip-da-máquina>:8080`.
- [ ] **Step 2:** Em duas máquinas (ou máquina + VM) com o RustDesk oficial apontando para o servidor (ID, chave, API `http://<ip>:8080`):
  - Login no app do "colaborador" com conta do painel → nome de exibição aparece no app.
  - Na máquina "cliente" (rodando **sem instalar** no Windows, se possível): aparece em "Não vinculados" no painel em até ~30 s; vincular a um cliente.
  - No app do colaborador: aba de catálogo mostra "Clientes PCEP" com "Cliente – Computador"; busca pelo nome do cliente funciona; conectar → janela de aceite no cliente mostra **"Maria – PCEP"**.
  - Aceitar, usar, encerrar → histórico mostra "Conectado" com duração e colaborador certo. Recusar uma tentativa → "Não concluída".
  - Desativar a colaboradora no painel → app desloga na próxima verificação.
  - Confirmar se o app portátil (não instalado) envia heartbeat; registrar o resultado.
- [ ] **Step 3:** Registrar em `docs/superpowers/plans/2026-11-04-validacao-e2e.md` o que foi verificado, versões usadas e qualquer divergência do contrato.
- [ ] **Step 4: Revisão final pelo `auditor`** (entrega crítica: autenticação, permissões, tokens) sobre o diff completo; corrigir achados bloqueantes (máx. 2 rodadas).
- [ ] **Step 5: Commit** (com autorização): `docs: validação ponta a ponta`
