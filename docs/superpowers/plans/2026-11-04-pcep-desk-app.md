# Pc&P Desk — Plano 2: Apps (fork do RustDesk)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gerar, a partir do fork `DevPCEP/pcep-desk-app`, duas variantes do app — **Pc&P Desk** (cliente: só recebe, aceite por clique travado, roda sem instalar) e **Pc&P Desk Equipe** (colaborador: completo, só conecta com login) — com servidor, chave e API gravados na compilação, para Windows, Linux e macOS, publicadas como release no GitHub; e uma página de download no painel.

**Architecture:** Toda decisão do PCEP fica num crate novo e puro, `libs/pcep` (testável localmente, sem dependências pesadas), que produz o mesmo JSON que o `custom.txt` assinado do RustDesk produziria. O código do upstream recebe só "ganchos" finos: aplicar esse JSON na inicialização (reaproveitando `read_custom_client` sem a verificação de assinatura), nome de exibição, bloqueio de conexão de saída e remoção de links do rustdesk.com. Diferenças por variante em arquivos de empacotamento (nome do produto, binário, identificador) são aplicadas por um script antes da compilação. A compilação roda num workflow próprio do GitHub Actions; nada pesado compila nesta máquina.

**Tech Stack:** Rust (crate `pcep` com `serde_json`, `base64`), Flutter/Dart (UI do RustDesk), bash, GitHub Actions, librsvg (`rsvg-convert`), ImageMagick, `png2icns` (icnsutils). Base: `rustdesk/rustdesk` master em 55786aa (versão 1.5.1).

**Spec:** `docs/superpowers/specs/2026-11-04-pcep-desk-design.md` (repositório `DevPCEP/pcep-desk`).

**Repositórios:** o código desta etapa vai em `DevPCEP/pcep-desk-app` (clone local em `/home/franklin/orca/Idealizados/pcep-desk-app`, branch `master`, remoto `upstream` = `rustdesk/rustdesk`). A Task 9 é no repositório do painel (`DevPCEP/pcep-desk`).

## Global Constraints

- Idioma de textos novos: português do Brasil. Comentários do código novo em português.
- Variantes (valores exatos):
  | | cliente | colaborador |
  |---|---|---|
  | `PCEP_VARIANTE` | `cliente` | `colaborador` |
  | APP_NAME (técnico, ASCII) | `PcepDesk` | `PcepDeskEquipe` |
  | Nome de exibição | `Pc&P Desk` | `Pc&P Desk Equipe` |
  | Nome do binário | `pcep-desk` | `pcep-desk-equipe` |
  | Identificador (macOS/Linux) | `br.com.pcepcontabil.desk` | `br.com.pcepcontabil.desk.equipe` |
- Valores embutidos vêm de variáveis de ambiente **na compilação**: `PCEP_VARIANTE`, `PCEP_SERVIDOR` (host ou host:porta do hbbs), `PCEP_CHAVE` (conteúdo de `id_ed25519.pub`, base64 de 32 bytes), `PCEP_API` (URL `http://` ou `https://` do painel). No CI vêm de *Repository variables* (`vars.PCEP_SERVIDOR`, `vars.PCEP_CHAVE`, `vars.PCEP_API`) — não são segredos.
- Um build de release **nunca** pode sair sem esses valores ou apontando para `rustdesk.com` (anti-objetivo da spec): o workflow falha antes de compilar.
- Aceite: `approve-mode = click` fixado (OVERWRITE) nas duas variantes; o cliente também fixa `allow-only-conn-window-open = Y`. Sem senha permanente no cliente.
- Verificação de atualização do upstream desligada (`enable-check-update = N`, `allow-auto-update = N`).
- Mudanças no código do upstream: mínimas, cada uma marcada com o comentário `// PCEP:` (ou `// PCEP:` em Dart), para facilitar merges futuros com `upstream/master`.
- Não alterar o submódulo `libs/hbb_common`.
- Link para o código-fonte (AGPL) visível no app: `https://github.com/DevPCEP/pcep-desk-app`.
- Commits autorizados pelo usuário por task. **Push** é feito pelo usuário (o Claude Code bloqueia push para repositório público) — tasks que precisam do CI terminam pedindo o push.

## Review Focus

1. **Build de release sem as variáveis** (`PCEP_SERVIDOR` vazio, chave inválida) → o workflow falha com mensagem clara antes de compilar; nunca gera app apontando para o servidor público. Teste na Task 1 (`validate_rejects_*`) e na Task 4 (passo de validação do workflow).
2. **Cliente tenta mudar servidor, senha ou modo de aceite** pela interface → impossível: configurações desabilitadas (`disable-settings`) e opções em OVERWRITE. Teste na Task 1 (`cliente_json_locks_settings`) + verificação manual na Task 10.
3. **Computador que já teve o RustDesk original com senha permanente** → o Pc&P Desk usa outro APP_NAME (outra pasta de configuração) e `approve-mode=click` fixado; senha antiga não abre acesso. Teste na Task 1 (`app_names_differ_from_rustdesk`) + item manual na Task 10.
4. **Colaborador sem login tenta conectar** por link `pcepdeskequipe://`, linha de comando ou botão → bloqueado no Rust (não só na tela). Teste na Task 1 (`check_outgoing_*`) + gancho na Task 7.
5. **Cliente instalado com a janela fechada** → conexão recusada (`allow-only-conn-window-open`). Teste na Task 1 (`cliente_json_requires_open_window`) + item manual na Task 10.

---

## Estrutura de arquivos (no fork)

```
pcep/
  LEIAME.md                 o que muda em relação ao RustDesk, como sincronizar com o upstream, como compilar
  aplicar-variante.sh       ajusta arquivos de empacotamento para a variante (Task 2)
  testes/aplicar-variante.test.sh
  icones/icone.svg          ícone do app (Task 3)
  icones/bandeja.svg        ícone da bandeja (monocromático)
  icones/gerar.sh           gera PNG/ICO/ICNS nos lugares que o RustDesk usa
libs/pcep/
  Cargo.toml  src/lib.rs    regras do PCEP (Task 1)
.github/workflows/pcep-build.yml   (Task 4)
Modificados (ganchos): Cargo.toml (dependência pcep), src/common.rs, src/lang.rs, src/flutter_ffi.rs,
  flutter/lib/main.dart, flutter/lib/common.dart, flutter/lib/desktop/pages/{desktop_home_page,connection_page,desktop_setting_page,install_page}.dart
```

---

### Task 1: Crate `libs/pcep` — regras do PCEP

**Files:**
- Create: `pcep/LEIAME.md`, `libs/pcep/Cargo.toml`, `libs/pcep/src/lib.rs`
- Modify: `Cargo.toml` (raiz: adicionar `pcep = { path = "libs/pcep" }` em `[dependencies]` e, se houver `[workspace] members`, incluir `libs/pcep`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Variant { Cliente, Colaborador }`; `impl Variant { pub fn parse(s: &str) -> Option<Variant>; pub fn app_name(self) -> &'static str; pub fn display_name(self) -> &'static str }` (valores da tabela das Global Constraints).
  - `pub struct RawValues<'a> { pub variant: Option<&'a str>, pub server: Option<&'a str>, pub key: Option<&'a str>, pub api: Option<&'a str> }`
  - `#[derive(Clone, Debug)] pub struct BuildValues { pub variant: Variant, pub server: String, pub key: String, pub api: String }`
  - `#[derive(Debug, PartialEq)] pub enum ConfigError { MissingVariant, UnknownVariant(String), MissingServer, MissingKey, InvalidKey, MissingApi, InvalidApi, PublicServer }` com `Display` em português.
  - `pub fn validate(raw: RawValues) -> Result<BuildValues, ConfigError>` — `trim` em tudo; variante ausente → `MissingVariant` (as outras checagens só depois); servidor ou API contendo `rustdesk.com` → `PublicServer`; chave deve decodificar (base64 padrão) para exatamente 32 bytes → senão `InvalidKey`; API deve começar com `http://` ou `https://` e não terminar com `/` (remover a barra final antes de validar).
  - `pub fn embedded() -> Option<Result<BuildValues, ConfigError>>` — lê `option_env!("PCEP_VARIANTE")`, `option_env!("PCEP_SERVIDOR")`, `option_env!("PCEP_CHAVE")`, `option_env!("PCEP_API")`; `None` quando `PCEP_VARIANTE` não foi definida (build de desenvolvimento = RustDesk normal).
  - `pub fn custom_client_json(b: &BuildValues) -> serde_json::Value` — mesmo formato do `custom.txt` do RustDesk (chaves de topo vão para HARD_SETTINGS; `override-settings` → OVERWRITE; `default-settings` → DEFAULT):
    - Ambas: `"app-name": app_name()`; `override-settings`: `custom-rendezvous-server = server`, `relay-server = server`, `api-server = api`, `key = key`, `approve-mode = "click"`, `enable-check-update = "N"`, `allow-auto-update = "N"`, `disable-change-id = "Y"`, `hide-server-settings = "Y"`, `hide-proxy-settings = "Y"`, `hide-websocket-settings = "Y"`.
    - Cliente, além disso: topo `"conn-type": "incoming"`, `"disable-settings": "Y"`, `"disable-account": "Y"`, `"disable-ab": "Y"`; `override-settings`: `allow-only-conn-window-open = "Y"`, `disable-change-permanent-password = "Y"`, `allow-logon-screen-password = "N"`.
  - `pub fn check_outgoing(variant: Variant, access_token: &str) -> Result<(), &'static str>` — Cliente → `Err("Este aplicativo só recebe conexões.")`; Colaborador com token vazio (após trim) → `Err("Faça login no Pc&P Desk Equipe para conectar.")`; senão `Ok(())`.

- [ ] **Step 1:** Clonar o fork: `git clone https://github.com/DevPCEP/pcep-desk-app /home/franklin/orca/Idealizados/pcep-desk-app && cd` nele, `git remote add upstream https://github.com/rustdesk/rustdesk.git`, `git submodule update --init --depth 1 libs/hbb_common`. Criar `pcep/LEIAME.md` (o que é o fork, tabela de variantes, variáveis `PCEP_*`, como sincronizar: `git fetch upstream && git merge upstream/master`, e que todo gancho é marcado com `PCEP:`).
- [ ] **Step 2: Testes que falham** em `libs/pcep/src/lib.rs` (`#[cfg(test)]`), com chave válida de teste = base64 de 32 bytes zero (`"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="`):
  - `validate_ok_trims_and_strips_slash`: api `" https://desk.exemplo.com.br/ "` → `"https://desk.exemplo.com.br"`.
  - `validate_rejects_missing_variant` (`MissingVariant`), `validate_rejects_unknown_variant` (`"tecnico"` → `UnknownVariant`), `validate_rejects_missing_server`, `validate_rejects_bad_key` (`"abc"` → `InvalidKey`; base64 de 31 bytes → `InvalidKey`), `validate_rejects_bad_api` (`"desk.exemplo"` → `InvalidApi`), `validate_rejects_public_server` (`"rs-ny.rustdesk.com"` → `PublicServer`; api `https://admin.rustdesk.com` → `PublicServer`).
  - `app_names_differ_from_rustdesk`: `Cliente.app_name() == "PcepDesk"`, `Colaborador.app_name() == "PcepDeskEquipe"`, nenhum igual a `"RustDesk"`, ambos só `[A-Za-z0-9_-]`.
  - `cliente_json_locks_settings`: `json["conn-type"] == "incoming"`, `json["disable-settings"] == "Y"`, `json["override-settings"]["approve-mode"] == "click"`, `["custom-rendezvous-server"] == server`, `["key"] == key`, `["api-server"] == api`, `["enable-check-update"] == "N"`.
  - `cliente_json_requires_open_window`: `["override-settings"]["allow-only-conn-window-open"] == "Y"`.
  - `colaborador_json_is_full_app`: sem `conn-type`, sem `disable-settings`, `approve-mode == "click"`, servidor/chave/api presentes.
  - `check_outgoing_cliente_blocked`, `check_outgoing_colaborador_requires_token` (`""` e `"   "` → Err; `"abc"` → Ok).
- [ ] **Step 3:** `cargo test -p pcep` → FAIL (não compila).
- [ ] **Step 4:** Implementar. Dependências: `serde_json` e `base64` (mesmas versões já usadas pelo workspace, ver `Cargo.lock`).
- [ ] **Step 5:** `cargo test -p pcep` → todos passam. (Se `cargo` exigir resolver o workspace inteiro e falhar por dependências de sistema, rodar com `cargo test --manifest-path libs/pcep/Cargo.toml` — o crate não pode depender de nada do workspace.)
- [ ] **Step 6: Commit:** `feat(pcep): crate com regras das variantes cliente e colaborador`

### Task 2: Script `pcep/aplicar-variante.sh` — empacotamento por variante

**Files:**
- Create: `pcep/aplicar-variante.sh`, `pcep/testes/aplicar-variante.test.sh`

**Interfaces:**
- Produces: `pcep/aplicar-variante.sh <cliente|colaborador>` executado na raiz do fork, idempotente; altera (com `sed`, valores da tabela das Global Constraints):
  - `Cargo.toml`: `[package.metadata.winres]` `OriginalFilename = "<binário>.exe"`, `ProductName = "<exibição>"`, `FileDescription = "<exibição>"`; `[package.metadata.bundle]` `name = "<exibição>"`, `identifier = "<identificador>"`.
  - `flutter/windows/runner/Runner.rc`: `CompanyName` = `PC&P Contabilidade` (no .rc, `&` precisa ser `&&`), `FileDescription`/`ProductName` = exibição (com `&&`), `InternalName` = binário, `OriginalFilename` = `<binário>.exe`.
  - `flutter/windows/CMakeLists.txt`: `BINARY_NAME` = binário.
  - `flutter/linux/CMakeLists.txt`: `BINARY_NAME` = binário, `APPLICATION_ID` = identificador.
  - `flutter/macos/Runner/Configs/AppInfo.xcconfig`: `PRODUCT_NAME` = `PcepDesk` ou `PcepDeskEquipe` (sem `&` no nome do .app), `PRODUCT_BUNDLE_IDENTIFIER` = identificador.
  - Sai com código 2 e mensagem de uso para argumento inválido.

- [ ] **Step 1: Teste que falha** `pcep/testes/aplicar-variante.test.sh`: copia os 6 arquivos alvo para um diretório temporário com a mesma estrutura, roda o script lá (via variável `PCEP_RAIZ`), e confere com `grep -F` cada valor esperado para `colaborador`; roda de novo (idempotência) e confere que nada duplicou; roda com `tecnico` e espera código 2.
- [ ] **Step 2:** `bash pcep/testes/aplicar-variante.test.sh` → FAIL.
- [ ] **Step 3:** Implementar (`set -euo pipefail`, raiz em `${PCEP_RAIZ:-.}`).
- [ ] **Step 4:** Teste passa para as duas variantes; `bash -n` limpo.
- [ ] **Step 5: Commit:** `feat(pcep): script que aplica a variante nos arquivos de empacotamento`

### Task 3: Ícones

**Files:**
- Create: `pcep/icones/icone.svg`, `pcep/icones/bandeja.svg`, `pcep/icones/gerar.sh`

**Decisões:**
- Fonte: `img/favicon.svg` e `img/logo.svg` do repositório `DevPCEP/pcep-desk` (copiar para `pcep/icones/origem/`).
- `icone.svg`: 1024×1024, quadrado com cantos arredondados (raio 224) na cor `#111528`, símbolo do `favicon.svg` em branco, centralizado, ocupando ~62% da largura.
- `bandeja.svg`: só o símbolo, preto, fundo transparente (no macOS é usado como *template*).
- `gerar.sh` (requer `rsvg-convert`, `convert`/ImageMagick e `png2icns`; checa e falha com mensagem se faltar): gera e sobrescreve `res/icon.png` (1024), `res/32x32.png`, `res/64x64.png`, `res/128x128.png`, `res/128x128@2x.png` (256), `res/mac-icon.png` (1024), `res/icon.ico` e `res/tray-icon.ico` (16, 32, 48, 64, 128, 256), `res/mac-tray-dark-x2.png` e `res/mac-tray-light-x2.png` (bandeja, 44×44), `flutter/windows/runner/resources/app_icon.ico`, `flutter/macos/Runner/AppIcon.icns`, `flutter/assets/icon.svg` (cópia de `icone.svg`), `flutter/assets/logo.png` (logo do escritório, 512 px de largura, a partir de `img/logo.svg` com `fill` escuro).
- Os arquivos gerados **não** são commitados: o CI roda `gerar.sh` antes de compilar (Task 4).

- [ ] **Step 1:** Criar `icone.svg` e `bandeja.svg` à mão (SVG válido; conferir com `xmllint --noout` se disponível, ou abrindo no navegador).
- [ ] **Step 2:** Criar `gerar.sh` com `set -euo pipefail` e a checagem de ferramentas; `bash -n` limpo. (Execução real acontece no CI da Task 4.)
- [ ] **Step 3: Commit:** `feat(pcep): ícones do Pc&P Desk e script de geração`
- [ ] **Step 4:** O controlador mostra `icone.svg` ao usuário (abrir no navegador) e ajusta se ele pedir.

### Task 4: Workflow `pcep-build.yml`

**Files:**
- Create: `.github/workflows/pcep-build.yml`

**Decisões:**
- Gatilhos: `workflow_dispatch` (com input `variante`: `ambas|cliente|colaborador`, padrão `ambas`) e `push` de tags `pcep-v*`.
- Job `validar` (ubuntu): falha se `vars.PCEP_SERVIDOR`, `vars.PCEP_CHAVE` ou `vars.PCEP_API` estiverem vazios, contiverem `rustdesk.com`, ou se a chave não decodificar para 32 bytes (`base64 -d | wc -c` = 32). Mensagem: "Configure as variáveis PCEP_SERVIDOR, PCEP_CHAVE e PCEP_API em Settings → Secrets and variables → Actions → Variables".
- Job `icones` (ubuntu): instala `librsvg2-bin imagemagick icnsutils`, roda `pcep/icones/gerar.sh`, publica os arquivos gerados como artifact `pcep-icones`.
- Jobs de build com matriz `variante × plataforma`, copiando (não chamando) os passos necessários de `.github/workflows/flutter-build.yml` para: Windows x86_64 (`build-for-windows-flutter`), Linux x86_64 (`build-rustdesk-linux` + `build-appimage`), macOS x86_64 e aarch64 (`build-for-macOS`). Antes de compilar cada job: baixar `pcep-icones` sobre a árvore, rodar `pcep/aplicar-variante.sh <variante>`, exportar `PCEP_VARIANTE`, `PCEP_SERVIDOR`, `PCEP_CHAVE`, `PCEP_API` no `env`.
- Remover/pular: assinatura (não há certificados — anotar no LEIAME como pendência), Android, iOS, web, sciter, flatpak, update metadata.
- Entregáveis (nomes fixos, sem versão, para links estáveis do painel):
  - Windows: `PcepDesk-Windows-x64.exe` (cliente) e `PcepDeskEquipe-Windows-x64.exe` (colaborador) — o executável portátil único que o upstream gera com `libs/portable`.
  - Linux: `PcepDesk-Linux-x86_64.AppImage` (cliente), `PcepDeskEquipe-Linux-x86_64.deb` (colaborador).
  - macOS: `PcepDesk-macOS-<arch>.dmg`, `PcepDeskEquipe-macOS-<arch>.dmg` (`arch` = `x64` ou `arm64`), sem assinatura.
- Em tag `pcep-v*`: criar release com todos os entregáveis (`softprops/action-gh-release`, como o upstream) e texto "Código-fonte: https://github.com/DevPCEP/pcep-desk-app".

- [ ] **Step 1:** Escrever o workflow; validar sintaxe com `python3 -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" .github/workflows/pcep-build.yml` e, se disponível, `actionlint`.
- [ ] **Step 2: Commit:** `ci(pcep): workflow de build das variantes`
- [ ] **Step 3:** Pedir ao usuário (via controlador): configurar as 3 *Repository variables* (para teste local: `PCEP_SERVIDOR` = IP do servidor de teste na rede, `PCEP_CHAVE` = conteúdo do `id_ed25519.pub`, `PCEP_API` = `http://<IP>:8080`), fazer push e disparar o workflow (`gh workflow run pcep-build.yml`).
- [ ] **Step 4:** Acompanhar com `gh run watch`/`gh run view --log-failed`; corrigir até todos os jobs passarem. Nesta task o app ainda é o RustDesk com ícones/nomes trocados (os ganchos chegam nas Tasks 5–8) — o objetivo é provar o pipeline.

### Task 5: Gancho — aplicar a configuração embutida

**Files:**
- Modify: `src/common.rs` (`read_custom_client` ~2593, `load_custom_client` ~2495)

**Interfaces:**
- Produces: `pub fn apply_custom_client_data(data: HashMap<String, serde_json::Value>)` em `src/common.rs` (o corpo atual de `read_custom_client` após `sign::verify` + parse, sem mudança de comportamento; `read_custom_client` passa a chamá-la). `pub fn load_pcep_config()` em `src/common.rs`: `match pcep::embedded()` → `None`: nada; `Some(Err(e))`: `log::error!` e `std::process::exit(1)` (build quebrado nunca roda com servidor errado); `Some(Ok(v))`: converte `pcep::custom_client_json(&v)` em `HashMap` e chama `apply_custom_client_data`.
- `load_custom_client()` chama `load_pcep_config()` **depois** de tentar o `custom.txt` (assim o PCEP sempre prevalece), com comentário `// PCEP:`.

- [ ] **Step 1:** Refatorar e adicionar as funções (diff mínimo).
- [ ] **Step 2:** `cargo test -p pcep` continua passando; push (usuário) e CI verde.
- [ ] **Step 3: Verificação no artefato do CI** (Linux AppImage do cliente, ou Windows exe numa VM): abre sem pedir servidor; Configurações indisponíveis; aparece em "Não vinculados" no painel de teste.
- [ ] **Step 4: Commit:** `feat(pcep): aplica configuração embutida na inicialização`

### Task 6: Gancho — nome de exibição "Pc&P Desk"

**Files:**
- Modify: `src/common.rs` (nova fn), `src/lang.rs:268-281`, `src/flutter_ffi.rs` (nova fn FFI), `flutter/lib/main.dart:499-500`, `flutter/lib/common.dart:3064,4230`

**Interfaces:**
- Produces: `pub fn get_app_display_name() -> String` em `src/common.rs` = `pcep::embedded()` válido → `variant.display_name()`; senão `get_app_name()`. `pub fn main_get_app_display_name_sync() -> SyncReturn<String>` em `src/flutter_ffi.rs` (regerar a ponte flutter_rust_bridge conforme o upstream faz no CI).
- `src/lang.rs`: a troca `"RustDesk"` → nome passa a usar `get_app_display_name()`.
- Dart: títulos de janela e textos que hoje usam `mainGetAppNameSync()` para **exibição** passam a usar `mainGetAppDisplayNameSync()` (não trocar usos de caminho/identificação).

- [ ] **Step 1:** Implementar; conferir com `grep -n "mainGetAppNameSync" flutter/lib` que só usos de exibição mudaram.
- [ ] **Step 2:** CI verde; no artefato, título da janela e textos ("Instalar Pc&P Desk") mostram o nome de exibição.
- [ ] **Step 3: Commit:** `feat(pcep): nome de exibição Pc&P Desk na interface`

### Task 7: Gancho — conexão de saída só com login (colaborador) e nunca no cliente

**Files:**
- Modify: `src/flutter_ffi.rs:150` (`session_add_sync`), `flutter/lib/common.dart:2602` (`connect`)

**Decisões:**
- Rust (fonte da verdade): em `session_add_sync`, antes de criar a sessão, se `pcep::embedded()` for válido, chamar `pcep::check_outgoing(variant, &LocalConfig::get_option("access_token"))`; em `Err(msg)` retornar o erro como o próprio `session_add_sync` já retorna erros (string), com `// PCEP:`.
- Dart (experiência): em `connect()`, se a variante for colaborador (nova FFI `mainPcepVariantSync()` → `"cliente" | "colaborador" | ""`) e `!gFFI.userModel.isLogin`, mostrar diálogo "Faça login no Pc&P Desk Equipe para conectar." com botão "Entrar" que abre o login existente, e não conectar.

- [ ] **Step 1:** Implementar.
- [ ] **Step 2:** CI verde; no artefato do colaborador: sem login, tentar conectar → diálogo e nenhuma sessão; abrir `pcepdeskequipe://<id>` sem login → erro, sem sessão; com login → conecta (janela de aceite no cliente mostra o nome).
- [ ] **Step 3: Commit:** `feat(pcep): colaborador só conecta com login`

### Task 8: Gancho — remover links do RustDesk e mostrar o código-fonte

**Files:**
- Modify: `flutter/lib/desktop/pages/desktop_home_page.dart` (~430-454, 535-552), `flutter/lib/desktop/pages/connection_page.dart:45`, `flutter/lib/desktop/pages/desktop_setting_page.dart:2620-2628`, `flutter/lib/desktop/pages/install_page.dart:236-238`

**Decisões:**
- Quando a variante PCEP estiver ativa (`mainPcepVariantSync() != ""`): esconder cards/links para `rustdesk.com` (download, pricing, docs, privacy); na página "Sobre" das Configurações, trocar o link do site por "Código-fonte: https://github.com/DevPCEP/pcep-desk-app" e manter a menção à licença AGPL-3.0 e ao projeto original RustDesk (atribuição exigida pela licença).
- Sem a variante (build de desenvolvimento), comportamento original.

- [ ] **Step 1:** Implementar; `grep -rn "rustdesk.com" flutter/lib/desktop` e conferir que todos os restantes estão atrás da checagem ou são comentários.
- [ ] **Step 2:** CI verde; conferir no artefato.
- [ ] **Step 3: Commit:** `feat(pcep): remove links do RustDesk e mostra o código-fonte`

### Task 9: Painel — página de download (repositório `DevPCEP/pcep-desk`)

**Files:**
- Create: `painel/src/web/downloads.rs`, `painel/templates/baixar.html`, `painel/tests/web_downloads.rs`
- Modify: `painel/src/web/mod.rs`, `painel/templates/base.html` (menu "Baixar"), títulos `PCEP Desk` → `Pc&P Desk` nos templates

**Decisões:**
- `GET /baixar` **público** (sem login), para o colaborador mandar o link ao cliente: seção "Para clientes (Pc&P Desk)" com Windows/macOS (Intel e Apple Silicon)/Linux; seção "Para a equipe (Pc&P Desk Equipe)" com os mesmos; links `https://github.com/DevPCEP/pcep-desk-app/releases/latest/download/<nome do entregável>` (nomes exatos da Task 4); instruções curtas: Windows "Se aparecer 'O Windows protegeu o computador', clique em Mais informações → Executar assim mesmo"; macOS "clique com o botão direito → Abrir na primeira vez"; link "Código-fonte".
- Seguir padrões do painel (Askama, CSS existente).

- [ ] **Step 1: Testes que falham** (`tests/web_downloads.rs`): `download_page_public` (200 sem cookie), `download_links_point_to_latest_release` (contém `releases/latest/download/PcepDesk-Windows-x64.exe` e `PcepDeskEquipe-Windows-x64.exe`), `download_page_has_source_link`.
- [ ] **Step 2:** `cargo test --test web_downloads` → FAIL. **Step 3:** implementar. **Step 4:** suíte completa + clippy limpos.
- [ ] **Step 5: Commit:** `feat(web): página de download do Pc&P Desk`

### Task 10: Validação com os apps próprios

- [ ] **Step 1:** Criar tag `pcep-v1.5.1-1` (usuário faz o push da tag) e conferir a release com todos os entregáveis.
- [ ] **Step 2:** Atualizar `docs/superpowers/plans/2026-11-04-validacao-e2e.md` (repositório do painel) com a "Parte C — apps Pc&P Desk", checklist manual na rede local (máquina + VM), incluindo os itens do Review Focus: cliente sem Configurações; aceite só por clique; RustDesk original com senha permanente na mesma VM não dá acesso ao Pc&P Desk; colaborador sem login não conecta (botão e link `pcepdeskequipe://`); cliente instalado com janela fechada recusa; nome "Pc&P Desk Equipe"/"Pc&P Desk" nas janelas; ícone; página `/baixar` baixa os arquivos certos.
- [ ] **Step 3:** Revisão final pelo `auditor` (entrega crítica: configuração de segurança do aceite e do acesso).
- [ ] **Step 4: Commit** (repo do painel): `docs: validação dos apps Pc&P Desk`
