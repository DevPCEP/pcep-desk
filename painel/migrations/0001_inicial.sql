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
