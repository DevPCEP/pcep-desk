#!/usr/bin/env bash
# Smoke test ponta a ponta do PCEP Desk, sem Docker, nesta máquina.
#
# Sobe o hbbs/hbbr 1.1.16 (binários do release oficial) e o painel com banco
# temporário e simula, com curl, o fluxo que o app RustDesk faz contra a API.
# Não substitui o teste manual com duas máquinas (veja
# docs/superpowers/plans/2026-11-04-validacao-e2e.md).
#
# Uso: servidor/smoke-test.sh
# Requisitos: bash, curl, unzip, jq, ss, cargo. Precisa de internet na primeira
# vez (baixa o rustdesk-server) e das portas 21115-21119 e 18080 livres.
set -euo pipefail

RUSTDESK_VERSAO="1.1.16"
RUSTDESK_URL="https://github.com/rustdesk/rustdesk-server/releases/download/${RUSTDESK_VERSAO}/rustdesk-server-linux-amd64.zip"
PAINEL_PORTA=18080
PAINEL_BASE="http://127.0.0.1:${PAINEL_PORTA}"

AQUI="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PAINEL_DIR="$(cd "${AQUI}/../painel" && pwd)"

# Dados da simulação.
SENHA_ADMIN="senha-admin-smoke1"
SENHA_MARIA="senha-maria-smoke1"
ID_COLAB="987654321"   # "computador da colaboradora"
ID_CLIENTE="111222333" # "computador do cliente"
CLIENTE_NOME="Padaria Smoke"
ROTULO="Caixa"
ALIAS_ESPERADO="${CLIENTE_NOME} – ${ROTULO}"
NOME_COLAB="Maria – PCEP"

TOTAL=0
FALHAS=0
PIDS=()
TMP=""

limpar() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    [ -n "${pid}" ] && kill "${pid}" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do
    [ -n "${pid}" ] && wait "${pid}" 2>/dev/null || true
  done
  if [ -n "${TMP}" ] && [ -d "${TMP}" ]; then
    rm -rf "${TMP}"
  fi
}
trap limpar EXIT
trap 'exit 130' INT TERM

ok() { TOTAL=$((TOTAL + 1)); echo "OK $1"; }
falhou() { TOTAL=$((TOTAL + 1)); FALHAS=$((FALHAS + 1)); echo "FALHOU $1"; }

# verifica "descrição" comando...  (OK se o comando retornar 0)
verifica() {
  local desc="$1"
  shift
  if "$@"; then ok "${desc}"; else falhou "${desc}"; fi
}

# Interrompe o teste (as verificações seguintes dependem do que falhou).
abortar() {
  echo "ABORTADO: $1" >&2
  FALHAS=$((FALHAS + 1))
  exit 1
}

# ---------------------------------------------------------------- requisitos
for cmd in curl unzip jq ss cargo; do
  command -v "${cmd}" >/dev/null 2>&1 || abortar "comando necessário não encontrado: ${cmd}"
done

TMP="$(mktemp -d)"
mkdir -p "${TMP}/baixado" "${TMP}/rustdesk" "${TMP}/dados"

# ------------------------------------------------------------- portas livres
ocupadas="$(ss -H -ltnu | awk '{print $5}' | grep -E ':(2111[5-9]|'"${PAINEL_PORTA}"')$' || true)"
if [ -n "${ocupadas}" ]; then
  echo "As portas 21115-21119 e ${PAINEL_PORTA} precisam estar livres, mas estas estão em uso:" >&2
  echo "${ocupadas}" >&2
  echo "Pare o hbbs/hbbr (ou o processo que usa a porta) e rode de novo." >&2
  exit 2
fi

# --------------------------------------------------- rustdesk-server (hbbs/hbbr)
echo "== Baixando rustdesk-server ${RUSTDESK_VERSAO}"
curl -fsSL --retry 3 --max-time 180 -o "${TMP}/baixado/rustdesk-server.zip" "${RUSTDESK_URL}" \
  || abortar "falha ao baixar ${RUSTDESK_URL}"
unzip -q "${TMP}/baixado/rustdesk-server.zip" -d "${TMP}/baixado"
HBBS="$(find "${TMP}/baixado" -type f -name hbbs | head -n1)"
HBBR="$(find "${TMP}/baixado" -type f -name hbbr | head -n1)"
[ -n "${HBBS}" ] && [ -n "${HBBR}" ] || abortar "hbbs/hbbr não encontrados no zip"
chmod +x "${HBBS}" "${HBBR}"

echo "== Subindo hbbs e hbbr (-k _)"
(cd "${TMP}/rustdesk" && exec "${HBBR}" -k _ >"${TMP}/hbbr.log" 2>&1) &
PIDS+=("$!")
(cd "${TMP}/rustdesk" && exec "${HBBS}" -k _ >"${TMP}/hbbs.log" 2>&1) &
PIDS+=("$!")

# Espera as chaves e as portas.
for _ in $(seq 1 30); do
  [ -f "${TMP}/rustdesk/id_ed25519.pub" ] && ss -H -ltn | grep -q ':21116 ' && break
  sleep 0.5
done
verifica "hbbs gerou id_ed25519.pub" test -s "${TMP}/rustdesk/id_ed25519.pub"
verifica "hbbs escutando em 21116/tcp" bash -c "ss -H -ltn | grep -q ':21116 '"
verifica "hbbr escutando em 21117/tcp" bash -c "ss -H -ltn | grep -q ':21117 '"

# ------------------------------------------------------------------- painel
echo "== Compilando o painel (cargo build --release)"
(cd "${PAINEL_DIR}" && cargo build --release --quiet) || abortar "falha ao compilar o painel"
PAINEL_BIN="${PAINEL_DIR}/target/release/painel"
[ -x "${PAINEL_BIN}" ] || abortar "binário do painel não encontrado: ${PAINEL_BIN}"

export PAINEL_DB="sqlite://${TMP}/dados/painel.db"
export PAINEL_URL_PUBLICA="${PAINEL_BASE}"
export PAINEL_ENDERECO="127.0.0.1:${PAINEL_PORTA}"
export PAINEL_COOKIE_SEGURO="false"

echo "== Criando o administrador pela CLI"
(cd "${PAINEL_DIR}" && PAINEL_SENHA_ADMIN="${SENHA_ADMIN}" "${PAINEL_BIN}" criar-admin \
  --usuario admin --nome "Administrador") >"${TMP}/criar-admin.log" 2>&1 \
  || { cat "${TMP}/criar-admin.log" >&2; abortar "criar-admin falhou"; }
ok "administrador criado pela CLI"

echo "== Subindo o painel em ${PAINEL_BASE}"
(cd "${PAINEL_DIR}" && exec "${PAINEL_BIN}" >"${TMP}/painel.log" 2>&1) &
PIDS+=("$!")
for _ in $(seq 1 40); do
  curl -s -o /dev/null "${PAINEL_BASE}/entrar" && break
  sleep 0.25
done
curl -s -o /dev/null "${PAINEL_BASE}/entrar" || { cat "${TMP}/painel.log" >&2; abortar "painel não respondeu"; }
ok "painel respondendo em ${PAINEL_BASE}"

# ------------------------------------------------------------------ helpers
STATUS=""
CORPO="${TMP}/corpo"
CABECALHOS="${TMP}/cabecalhos"
JAR="${TMP}/cookies.txt"

# chama MÉTODO CAMINHO [corpo-json] [token]  -> STATUS, ${CORPO}, ${CABECALHOS}
chama() {
  local metodo="$1" caminho="$2" corpo="${3:-}" token="${4:-}"
  local args=(-s -o "${CORPO}" -D "${CABECALHOS}" -w '%{http_code}' -X "${metodo}")
  [ -n "${token}" ] && args+=(-H "Authorization: Bearer ${token}")
  if [ -n "${corpo}" ]; then
    args+=(-H 'Content-Type: application/json' --data "${corpo}")
  fi
  STATUS="$(curl "${args[@]}" "${PAINEL_BASE}${caminho}" || echo 000)"
}

# web MÉTODO CAMINHO [campos form...]  (usa o cookie jar do admin)
web() {
  local metodo="$1" caminho="$2"
  shift 2
  local args=(-s -o "${CORPO}" -D "${CABECALHOS}" -w '%{http_code}' -X "${metodo}" -b "${JAR}" -c "${JAR}")
  local campo
  for campo in "$@"; do args+=(--data-urlencode "${campo}"); done
  STATUS="$(curl "${args[@]}" "${PAINEL_BASE}${caminho}" || echo 000)"
}

cabecalho_location() { grep -i '^location:' "${CABECALHOS}" | tr -d '\r' | awk '{print $2}'; }

# ---------------------------------------------------------- painel web (admin)
echo "== Painel web: login do administrador"
web POST /entrar "usuario=admin" "senha=${SENHA_ADMIN}"
verifica "login web do administrador (303, cookie de sessão)" \
  bash -c "[ '${STATUS}' = 303 ] && grep -q pcep_sessao '${JAR}'"

web GET /clientes
verifica "página /clientes abre com a sessão (200)" test "${STATUS}" = 200

web POST /colaboradores "usuario=maria" "nome=${NOME_COLAB}" "email=" "senha=${SENHA_MARIA}"
verifica "colaboradora criada pelo painel (303)" test "${STATUS}" = 303

# --------------------------------------------- computador do cliente (sem app)
echo "== Computador do cliente: heartbeat e sysinfo"
chama POST /api/heartbeat "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"ver\":\"1004005\"}"
verifica "heartbeat do cliente (200, corpo {})" bash -c "[ '${STATUS}' = 200 ] && [ \"\$(cat '${CORPO}')\" = '{}' ]"

chama POST /api/sysinfo "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"hostname\":\"PC-CAIXA\",\"os\":\"Windows / 11 Pro\",\"username\":\"caixa\",\"cpu\":\"i5\",\"memory\":\"8GB\",\"version\":\"1.4.5\"}"
verifica "sysinfo do cliente (200, SYSINFO_UPDATED)" \
  bash -c "[ '${STATUS}' = 200 ] && [ \"\$(cat '${CORPO}')\" = 'SYSINFO_UPDATED' ]"

web GET /nao-vinculados
verifica "computador do cliente aparece em Não vinculados" \
  bash -c "[ '${STATUS}' = 200 ] && grep -qF 'PC-CAIXA' '${CORPO}' && grep -qF '${ID_CLIENTE}' '${CORPO}'"

web POST /clientes "nome=${CLIENTE_NOME}" "documento=" "observacao="
CLIENTE_URL="$(cabecalho_location || true)"
verifica "cliente criado (303 para /clientes/<id>)" \
  bash -c "[ '${STATUS}' = 303 ] && [[ '${CLIENTE_URL}' =~ ^/clientes/[0-9]+\$ ]]"
CLIENTE_ID="${CLIENTE_URL##*/}"

web POST /nao-vinculados/vincular "cliente_id=${CLIENTE_ID}" "codigo=111 222 333" "rotulo=${ROTULO}" "observacao="
verifica "computador vinculado ao cliente (303)" test "${STATUS}" = 303

web GET /nao-vinculados
verifica "computador some de Não vinculados após vincular" \
  bash -c "[ '${STATUS}' = 200 ] && ! grep -qF 'PC-CAIXA' '${CORPO}'"

web GET "/clientes/${CLIENTE_ID}"
verifica "detalhe do cliente mostra o computador e o rótulo" \
  bash -c "[ '${STATUS}' = 200 ] && grep -qF 'data-copiar=\"${ID_CLIENTE}\"' '${CORPO}' && grep -qF '${ROTULO}' '${CORPO}'"

# ------------------------------------------- app da colaboradora (contrato API)
echo "== App da colaboradora: login e catálogo"
chama POST /api/login "{\"username\":\"maria\",\"password\":\"${SENHA_MARIA}\",\"id\":\"${ID_COLAB}\",\"uuid\":\"dXVpZC1jb2xhYg==\",\"type\":\"account\"}"
verifica "login no app (200, access_token)" \
  bash -c "[ '${STATUS}' = 200 ] && jq -e '.type == \"access_token\" and (.access_token|length) > 0' '${CORPO}' >/dev/null"
TOKEN="$(jq -r '.access_token // empty' "${CORPO}")"
[ -n "${TOKEN}" ] || abortar "login no app não devolveu token"
verifica "login devolve o nome de exibição \"${NOME_COLAB}\"" \
  bash -c "jq -e '.user.display_name == \"${NOME_COLAB}\"' '${CORPO}' >/dev/null"

chama POST /api/currentUser "{\"id\":\"${ID_COLAB}\",\"uuid\":\"dXVpZC1jb2xhYg==\"}" "${TOKEN}"
verifica "currentUser com token (200, display_name)" \
  bash -c "[ '${STATUS}' = 200 ] && jq -e '.display_name == \"${NOME_COLAB}\"' '${CORPO}' >/dev/null"

chama POST /api/ab/personal "" "${TOKEN}"
verifica "ab/personal devolve o catálogo pessoal (guid)" \
  bash -c "[ '${STATUS}' = 200 ] && jq -e '(.guid|length) > 0' '${CORPO}' >/dev/null"

chama POST "/api/ab/shared/profiles?current=1&pageSize=20" "" "${TOKEN}"
verifica "ab/shared/profiles lista \"Clientes PCEP\" (clientes-pcep, somente leitura)" \
  bash -c "[ '${STATUS}' = 200 ] && jq -e '.data[0].guid == \"clientes-pcep\" and .data[0].name == \"Clientes PCEP\" and .data[0].rule == 1' '${CORPO}' >/dev/null"

chama POST "/api/ab/peers?ab=clientes-pcep&current=1&pageSize=100" "" "${TOKEN}"
verifica "ab/peers do catálogo compartilhado traz o alias \"${ALIAS_ESPERADO}\"" \
  bash -c "[ '${STATUS}' = 200 ] && jq -e '.data[] | select(.id == \"${ID_CLIENTE}\" and .alias == \"${ALIAS_ESPERADO}\")' '${CORPO}' >/dev/null"

# ------------------------------------------------- histórico (audit do cliente)
echo "== Histórico: conexão aceita e conexão recusada"
chama POST /api/audit/conn "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"conn_id\":7,\"session_id\":1,\"action\":\"new\",\"ip\":\"200.1.2.3\"}"
verifica "audit new (200, corpo vazio)" bash -c "[ '${STATUS}' = 200 ] && [ ! -s '${CORPO}' ]"
chama POST /api/audit/conn "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"conn_id\":7,\"session_id\":1,\"peer\":[\"${ID_COLAB}\",\"${NOME_COLAB}\"],\"type\":0}"
verifica "audit authorized (200, corpo vazio)" bash -c "[ '${STATUS}' = 200 ] && [ ! -s '${CORPO}' ]"
sleep 1
chama POST /api/audit/conn "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"conn_id\":7,\"session_id\":1,\"action\":\"close\"}"
verifica "audit close (200, corpo vazio)" bash -c "[ '${STATUS}' = 200 ] && [ ! -s '${CORPO}' ]"

# Tentativa recusada: new e close, sem authorized.
chama POST /api/audit/conn "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"conn_id\":8,\"session_id\":2,\"action\":\"new\",\"ip\":\"200.1.2.3\"}"
chama POST /api/audit/conn "{\"id\":\"${ID_CLIENTE}\",\"uuid\":\"dXVpZC1jbGllbnRl\",\"conn_id\":8,\"session_id\":2,\"action\":\"close\"}"
verifica "audit de tentativa recusada (new + close) aceito" test "${STATUS}" = 200

web GET /historico
verifica "histórico mostra \"Conectado\" com o nome da colaboradora" \
  bash -c "[ '${STATUS}' = 200 ] && grep -qF 'Conectado' '${CORPO}' && grep -qF '${NOME_COLAB}' '${CORPO}'"
verifica "histórico mostra \"Não concluída\" para a tentativa recusada" \
  bash -c "grep -qF 'Não concluída' '${CORPO}'"

# ----------------------------------------------------- desativar a colaboradora
echo "== Desativar a colaboradora"
web GET /colaboradores
# A lista traz um link de edição por colaborador; abre cada um até achar a "maria".
MARIA_ID="$(for id in $(grep -oE '/colaboradores/[0-9]+/editar' "${CORPO}" | sed -n 's#.*/colaboradores/\([0-9]*\)/editar#\1#p'); do
  web GET "/colaboradores/${id}/editar"
  if grep -qF 'value="maria"' "${CORPO}"; then echo "${id}"; fi
done | head -n1)"
[ -n "${MARIA_ID}" ] || abortar "id da colaboradora não encontrado no painel"

web POST "/colaboradores/${MARIA_ID}/desativar"
verifica "colaboradora desativada pelo painel (303)" test "${STATUS}" = 303

chama POST /api/currentUser "{\"id\":\"${ID_COLAB}\",\"uuid\":\"dXVpZC1jb2xhYg==\"}" "${TOKEN}"
verifica "currentUser com o token antigo responde 401 após desativar" test "${STATUS}" = 401

chama POST /api/login "{\"username\":\"maria\",\"password\":\"${SENHA_MARIA}\",\"id\":\"${ID_COLAB}\",\"uuid\":\"dXVpZC1jb2xhYg==\",\"type\":\"account\"}"
# O app lê o erro de login no corpo (campo "error"), não no status HTTP.
verifica "novo login da colaboradora desativada é recusado (sem access_token, erro \"Usuário desativado\")" \
  bash -c "jq -e '(has(\"access_token\") | not) and .error == \"Usuário desativado\"' '${CORPO}' >/dev/null"

# ------------------------------------------------------------------- resumo
echo
echo "== Resumo: ${TOTAL} verificações, ${FALHAS} falha(s)"
if [ "${FALHAS}" -ne 0 ]; then
  echo "Logs (últimas linhas) do painel:" >&2
  tail -n 20 "${TMP}/painel.log" >&2 || true
  exit 1
fi
echo "Tudo certo."
