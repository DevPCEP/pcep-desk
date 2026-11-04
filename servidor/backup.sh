#!/usr/bin/env bash
# Backup do PCEP Desk: banco do painel + chaves do servidor RustDesk. Mantém 14 dias.
# Rode SEMPRE como root (sudo ./backup.sh ou crontab do root): as chaves em dados/rustdesk
# pertencem ao root. Se uma etapa falhar, as demais continuam e o código de saída final é 1.
set -euo pipefail

cd "$(dirname "$0")"

CARIMBO="$(date +%Y%m%d-%H%M%S)"
PASTA="dados/painel/backups"   # no host; dentro do contêiner é /dados/backups
DIAS=14
FALHAS=0

# 1) Banco do painel: cópia consistente feita pelo próprio painel (VACUUM INTO).
#    A pasta é criada dentro do contêiner para ficar com o dono certo (PAINEL_UID).
if docker compose exec -T painel mkdir -p /dados/backups \
  && docker compose exec -T painel painel backup "/dados/backups/painel-${CARIMBO}.db"; then
  chmod 600 "$PASTA/painel-${CARIMBO}.db"
else
  echo "ERRO: não consegui gravar o backup do banco." >&2
  FALHAS=1
fi

# 2) Chaves do servidor RustDesk. Sem a chave, todos os clientes teriam de ser reconfigurados.
for f in id_ed25519 id_ed25519.pub; do
  origem="dados/rustdesk/$f"
  destino="$PASTA/${f/id_ed25519/id_ed25519-${CARIMBO}}"
  if cp "$origem" "$destino"; then
    chmod 600 "$destino"
  else
    echo "ERRO: não consegui copiar $origem (rode como root)." >&2
    FALHAS=1
  fi
done

# 3) Retenção: apaga backups com mais de $DIAS dias. Roda mesmo se as etapas acima falharem.
if [ -d "$PASTA" ]; then
  chmod 700 "$PASTA"
  find "$PASTA" -maxdepth 1 -type f \( -name 'painel-*.db' -o -name 'id_ed25519-*' \) -mtime "+${DIAS}" -delete
fi

if [ "$FALHAS" -ne 0 ]; then
  echo "Backup terminou com ERROS (veja as mensagens acima)." >&2
  exit 1
fi
echo "Backup concluído: $PASTA (carimbo ${CARIMBO})"
