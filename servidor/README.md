# PCEP Desk: servidor e painel

Este diretório tem tudo o que é preciso para colocar no ar:

- o **servidor de conexão** do RustDesk (`hbbs` e `hbbr`), que registra os computadores e retransmite as conexões;
- o **painel** do escritório (cadastro de clientes, computadores, colaboradores e histórico);
- o **Caddy**, que serve o painel em HTTPS com certificado automático.

> **Importante:** até o aplicativo próprio do PCEP Desk (Plano 2) ficar pronto, os testes são feitos com o **RustDesk original**, apontado para este servidor (veja a seção "Testando com o RustDesk original").

## 1. Requisitos

- Um servidor Linux (VPS ou máquina no escritório) com **Docker** e **Docker Compose v2** instalados (`docker compose version` deve funcionar).
- Um **domínio** (por exemplo `desk.pcepcontabil.com.br`) cujo DNS você consiga editar.
- `git` para baixar e atualizar o projeto.

## 2. Portas a liberar

Libere no firewall do servidor (e, se for o servidor do escritório, também no roteador):

| Porta | Protocolo | Para quê |
|---|---|---|
| 21115 a 21119 | TCP | RustDesk (ID, relay e clientes web) |
| 21116 | UDP | RustDesk (registro e descoberta) |
| 80 e 443 | TCP | Painel em HTTPS (o 80 é usado para emitir o certificado) |

## 3. DNS

- **VPS:** crie um registro **A** do domínio (`desk.pcepcontabil.com.br`) apontando para o IP da VPS.
- **Servidor no escritório:**
  - se o provedor der **IP fixo**, crie um registro **A** para esse IP;
  - se o IP mudar, use um serviço de **DDNS** (por exemplo No-IP ou DuckDNS), que dá um nome como `escritorio.ddns.net`, e crie um registro **CNAME** do seu domínio para esse nome;
  - no **roteador**, faça o redirecionamento (port forwarding) das portas da tabela acima para o IP interno do servidor, e deixe esse IP interno fixo.

Confira antes de continuar: `ping desk.pcepcontabil.com.br` deve mostrar o IP certo. Sem o DNS correto o Caddy não consegue emitir o certificado.

## 4. Configuração

```bash
git clone <endereço-do-repositório> pcep-desk
cd pcep-desk/servidor
cp .env.example .env
nano .env        # ajuste o domínio e a URL pública
```

No `.env`:

- `PAINEL_DOMINIO` e `PAINEL_URL_PUBLICA`: o seu domínio (a URL leva `https://`).
- `PAINEL_UID` e `PAINEL_GID`: o resultado de `id -u` e `id -g` do usuário com que você roda o Docker. Isso evita problemas de permissão na pasta de dados.
- Deixe `PAINEL_COOKIE_SEGURO=true`.

Crie as pastas de dados **antes** de subir (com o seu usuário, sem `sudo`):

```bash
mkdir -p dados/painel/backups dados/rustdesk
```

## 5. Subir o sistema

```bash
docker compose up -d --build
docker compose ps          # os quatro serviços devem estar "running"
```

A primeira vez demora, porque o painel é compilado. Para ver os logs: `docker compose logs -f painel`.

## 6. Criar o administrador

```bash
docker compose exec painel painel criar-admin --usuario franklin --nome "Franklin"
```

O comando pede a senha (digite e tecle Enter). Depois, abra `https://<seu-domínio>` no navegador e entre com esse usuário. Os demais colaboradores são cadastrados dentro do painel.

## 7. Onde fica a chave pública

Na primeira execução o `hbbs` cria o par de chaves do servidor. A **chave pública** fica em:

```
servidor/dados/rustdesk/id_ed25519.pub
```

Veja o conteúdo com `cat dados/rustdesk/id_ed25519.pub`. Esse texto é o que os aplicativos precisam no campo **Chave**. A chave **privada** (`id_ed25519`) nunca deve ser compartilhada.

## 8. Testando com o RustDesk original

No RustDesk original (instalado em um computador de teste), abra **Configurações → Rede** (pode ser preciso clicar no cadeado para desbloquear) e preencha:

- **Servidor de ID** e **Servidor de relay**: o domínio, por exemplo `desk.pcepcontabil.com.br`;
- **Servidor de API**: `https://desk.pcepcontabil.com.br`;
- **Chave**: o conteúdo do arquivo `id_ed25519.pub`.

Depois clique em **OK**. Se o indicador de status na tela inicial ficar verde ("Pronto"), o computador está registrado no seu servidor. Para entrar com a conta do painel, use o login do RustDesk (usuário e senha cadastrados no painel).

## 9. Backup e restauração

### Fazer o backup

```bash
./backup.sh
```

O script grava, em `dados/painel/backups/`, uma cópia do banco (`painel-AAAAMMDD-HHMMSS.db`) e das chaves do servidor RustDesk (`id_ed25519-AAAAMMDD-HHMMSS` e `.pub`), e apaga o que tiver mais de 14 dias. Se ele reclamar de permissão ao copiar a chave, rode com `sudo ./backup.sh`.

Para rodar todo dia às 2h, adicione ao `crontab -e`:

```
0 2 * * * /caminho/para/pcep-desk/servidor/backup.sh >> /var/log/pcep-backup.log 2>&1
```

Copie também a pasta `dados/painel/backups/` para **outro lugar** (outro computador, nuvem): backup que fica só no servidor se perde junto com ele.

### Restaurar o banco

```bash
docker compose stop painel
rm -f dados/painel/painel.db-wal dados/painel/painel.db-shm
cp dados/painel/backups/painel-AAAAMMDD-HHMMSS.db dados/painel/painel.db
docker compose start painel
```

(Troque o nome do arquivo pelo backup que quer restaurar.) Para restaurar as chaves, pare o `hbbs` e o `hbbr`, copie `id_ed25519-...` para `dados/rustdesk/id_ed25519` e `id_ed25519-....pub` para `dados/rustdesk/id_ed25519.pub`, e suba de novo.

## 10. Atualização

```bash
cd pcep-desk
git pull
cd servidor
docker compose up -d --build
```

Os dados ficam na pasta `dados/` e não são perdidos na atualização. Faça um `./backup.sh` antes, por segurança.

## Problemas comuns

- **Caddy não emite o certificado:** confira o DNS (seção 3) e se as portas 80 e 443 estão liberadas.
- **Painel não grava no banco (erro de permissão):** confira `PAINEL_UID`/`PAINEL_GID` no `.env` e a dona da pasta `dados/painel` (`ls -ld dados/painel`). Depois `docker compose up -d --force-recreate painel`.
- **RustDesk não conecta:** confira a chave (sem espaços extras) e as portas da seção 2, inclusive a UDP 21116.
