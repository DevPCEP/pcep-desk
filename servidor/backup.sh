#!/usr/bin/env bash
# Backup do PCEP Desk: banco do painel + chaves do servidor RustDesk. Mantém 14 dias.
# Uso: ./backup.sh   (use sudo se dados/rustdesk pertencer ao root)
set -euo pipefail

cd "$(dirname "$0")"

CARIMBO="$(date +%Y%m%d-%H%M%S)"
PASTA="dados/painel/backups"   # no host; dentro do contêiner é /dados/backups
DIAS=14

mkdir -p "$PASTA"

# 1) Banco do painel: cópia consistente feita pelo próprio painel (VACUUM INTO).
docker compose exec -T painel painel backup "/dados/backups/painel-${CARIMBO}.db"

# 2) Chaves do servidor RustDesk. Sem a chave, todos os clientes teriam de ser reconfigurados.
for f in id_ed25519 id_ed25519.pub; do
  origem="dados/rustdesk/$f"
  destino="$PASTA/${f/id_ed25519/id_ed25519-${CARIMBO}}"
  if ! cp "$origem" "$destino"; then
    echo "ERRO: não consegui copiar $origem (rode com sudo?)." >&2
    exit 1
  fi
done
chmod 600 "$PASTA/id_ed25519-${CARIMBO}"

# 3) Apaga backups com mais de $DIAS dias.
find "$PASTA" -maxdepth 1 -type f \( -name 'painel-*.db' -o -name 'id_ed25519-*' \) -mtime "+${DIAS}" -delete

echo "Backup concluído: $PASTA (carimbo ${CARIMBO})"
