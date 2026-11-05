# Pc&P Desk — Acabamento (Plano 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fechar as pendências do Plano 2 antes da primeira publicação: pacote `.deb` do Linux com nomes próprios, assinatura do Windows preparada (liga sozinha quando houver certificado) e textos visíveis sem nome técnico (`PcepDesk`) nem "RustDesk".

**Architecture:** Tudo continua no fork `DevPCEP/pcep-desk-app` pelo mesmo mecanismo do Plano 2: o upstream fica intacto no repositório e `pcep/aplicar-variante.sh` reescreve os arquivos de empacotamento no CI; o código-fonte recebe ganchos `PCEP:` pequenos que trocam `get_app_name()` por `get_app_display_name()` só onde o texto é exibido. A assinatura é um script único (`pcep/assinar-windows.ps1`) chamado pelo workflow, que não faz nada quando os segredos não existem. O painel (`pcep-desk`) só ganha os links dos `.deb` na página `/baixar`.

**Tech Stack:** Bash + GNU sed, Python (build.py do upstream), GitHub Actions, PowerShell + signtool (Windows SDK do runner `windows-2022`), Rust, Dart/Flutter, C++ (runner Windows/Linux), Axum/Askama (painel).

**Spec:** `docs/superpowers/specs/2026-11-04-pcep-desk-design.md` e as pendências registradas em `.superpowers/sdd/2026-11-04-pcep-desk-app/progress.md` (Task 4: `.deb`; Task 6 e auditoria final: textos e título da janela; decisão do usuário em 2026-11-05: assinatura "deixar preparado").

## Global Constraints

- Nome de exibição: `Pc&P Desk` (cliente) e `Pc&P Desk Equipe` (colaborador). Nomes técnicos: `PcepDesk`/`PcepDeskEquipe`; binários e pacotes: `pcepdesk`/`pcepdeskequipe`.
- Nada de upstream editado à mão quando o arquivo é de empacotamento: as trocas vão em `pcep/aplicar-variante.sh` e são cobertas por `pcep/testes/aplicar-variante.test.sh`.
- Ganchos no código marcados com comentário `// PCEP:` (Rust/Dart/C++) ou `# PCEP:` (shell/Python/YAML).
- Segredos nunca aparecem em log, artefato ou commit. Os segredos de assinatura são `PCEP_ASSINATURA_PFX` (PFX em base64) e `PCEP_ASSINATURA_SENHA`; sem eles, o build segue sem assinar e registra `Assinatura: sem certificado, executável não assinado.`
- Entregáveis da release passam de 6 para 8: os 6 atuais + `PcepDesk-Linux-x86_64.deb` e `PcepDeskEquipe-Linux-x86_64.deb`.
- Verificação local possível: `bash pcep/testes/aplicar-variante.test.sh`, `cargo test -p pcep --lib` (fork) e `cargo test` em `painel/`. A compilação real dos apps só acontece no CI (`gh workflow run pcep-build.yml -f variante=ambas`, disparado pelo usuário).
- Push e disparo de workflow são feitos pelo usuário com `!`. Commits locais estão autorizados.

## Review Focus

1. Instalar o `.deb` do cliente numa máquina que já tem o RustDesk original instalado: os dois coexistem (pacote, serviço, `/usr/bin`, `.desktop`, `/usr/share/<binario>` diferentes) e remover um não remove o outro. Teste: Task 1, passo de instalação no CI confere que nenhum caminho instalado contém `rustdesk`.
2. Remover o `.deb` (`apt remove`): serviço parado e desabilitado, link `/usr/bin/<binario>` removido. Teste: Task 1, passo de remoção no CI.
3. Assinatura ligada com certificado inválido ou senha errada: o build falha com mensagem clara, sem expor a senha; nunca publica executável parcialmente assinado. Teste: Task 2, `assinar-windows.ps1` com senha errada sai com código ≠ 0.
4. Arquivos já assinados por terceiros (drivers da impressora, `usbmmidd`, DLLs com assinatura válida) não são reassinados. Teste: Task 2, o script ignora arquivos com `Get-AuthenticodeSignature` `Valid` e a pasta `drivers`.
5. Abrir um link `pcepdesk://` com o app já aberto no Windows continua levando à janela existente depois da troca do título (instância única). Teste: Task 4, item manual D.6 no roteiro (não há como testar no CI).

---

### Task 1: Pacote `.deb` do Linux com nomes próprios

**Files:**
- Modify: `pcep/aplicar-variante.sh` (bloco Linux, depois das trocas de `flutter/linux/CMakeLists.txt` e do AppImage)
- Modify: `pcep/testes/aplicar-variante.test.sh` (incluir os novos alvos em `ALVOS` e as asserções abaixo)
- Modify: `.github/workflows/pcep-build.yml` (job `linux` ~:420-636; job `release` ~:924-957)
- Modify: `pcep/LEIAME.md` (seção de empacotamento Linux; corrigir a linha 52, que diz que `libs/portable` ainda mostra RustDesk; isso já foi resolvido)

**Interfaces:**
- Consumes: variáveis já definidas em `aplicar-variante.sh` para a variante (`binario` = `pcepdesk|pcepdeskequipe`, nome técnico e nome de exibição; use os nomes que o script já tem).
- Produces: depois de `aplicar-variante.sh <variante>`, `python3 build.py --flutter` gera `<binario>-<versão>.deb` (Package `<binario>`), com `/usr/share/<binario>/<binario>`, `/usr/bin/<binario>`, `<binario>.service`, `/usr/share/applications/<binario>.desktop` e `<binario>-link.desktop`. O workflow publica o artifact `entregavel-<App>-Linux-x86_64.deb` e o job `release` exige 8 arquivos.

Decisões:
- O código Rust já espera esses nomes: `systemctl ... <app em minúsculas>` (`src/platform/linux.rs:2845-2880`), `.desktop` `<app>.desktop` (`linux.rs:3002-3007`) e config em `~/.config/<binario>` (`directories_next` coloca em minúsculas). Por isso a regra é uma troca única: `rustdesk` vira `<binario>` nos arquivos de pacote.
- As duas variantes têm serviço systemd (o app da equipe também roda `--service` hoje; tirar o serviço muda comportamento sem teste disponível).
- `RustDesk` como nome visível vira o nome de exibição (`Name=` dos `.desktop`, `Description=` do service, `Description:` do control).
- Control: `Package: <binario>`, `Maintainer: PCEP <dev@pcepcontabil.com.br>`, `Homepage: https://github.com/DevPCEP/pcep-desk-app`. Sem `Conflicts/Replaces` com `rustdesk`: os dois podem coexistir.

- [ ] **Step 1: Escrever as asserções que falham em `aplicar-variante.test.sh`**

Adicionar a `ALVOS`: `res/rustdesk.service`, `res/rustdesk.desktop`, `res/rustdesk-link.desktop`, `res/DEBIAN/postinst`, `res/DEBIAN/preinst`, `res/DEBIAN/prerm`, `res/DEBIAN/postrm`. Para cada variante (cliente → `pcepdesk`/`Pc&P Desk`; colaborador → `pcepdeskequipe`/`Pc&P Desk Equipe`), depois de rodar o script numa raiz temporária:
- existem `res/<binario>.service`, `res/<binario>.desktop` e `res/<binario>-link.desktop`;
- `grep -ri rustdesk` em `res/<binario>.service res/<binario>*.desktop res/DEBIAN/` não acha nada (exceto `libsciter-gtk.so`, que não contém a palavra);
- `res/<binario>.desktop` contém `Name=<nome de exibição>`, `Exec=<binario> %u`, `Icon=<binario>` e `StartupWMClass=<binario>`;
- `res/<binario>-link.desktop` contém `MimeType=x-scheme-handler/<binario>;`;
- `res/DEBIAN/postinst` contém `systemctl enable <binario>` e `ln -f -s /usr/share/<binario>/<binario> /usr/bin/<binario>`;
- `build.py` contém `Package: <binario>`, `Homepage: https://github.com/DevPCEP/pcep-desk-app`, `tmpdeb/usr/share/<binario>` e `res/<binario>.service`, e a função `build_flutter_deb` não contém mais `rustdesk` (extrair o corpo com `sed -n '/^def build_flutter_deb/,/^def /p'`);
- `appimage/AppImageBuilder-x86_64.yml` contém `usr/share/<binario>/<binario>`, `id: <binario>` e `name: <binario>`;
- a variante com CRLF (função `para_crlf` já existente) produz o mesmo resultado;
- rodar o script duas vezes não quebra (ele já falha se o alvo não existir; manter esse comportamento).

- [ ] **Step 2: Rodar e ver falhar**

Run: `bash pcep/testes/aplicar-variante.test.sh`
Expected: `FALHOU:` nas novas asserções; as antigas continuam passando.

- [ ] **Step 3: Implementar em `aplicar-variante.sh`**

Para `res/rustdesk.service`, `res/rustdesk.desktop` e `res/rustdesk-link.desktop`: gerar `res/<binario>.*` com `RustDesk` substituído pelo nome de exibição e `rustdesk` por `<binario>`, e apagar o original (o `build.py` reescrito passa a usar o novo nome). `res/DEBIAN/*`: `rustdesk` vira `<binario>`. `build.py`: dentro de `generate_control_file` e `build_flutter_deb`, `rustdesk` vira `<binario>`, mais Maintainer, Homepage e Description. Atenção: a função também copia `res/128x128@2x.png` para `.../icons/hicolor/256x256/apps/rustdesk.png` e `res/scalable.svg` para `rustdesk.svg`; o nome final vira `<binario>.png/.svg`. Restringir as trocas ao trecho dessas duas funções, com `sed` por intervalo de linhas. Depois de cada troca, conferir com `grep` e falhar se não houver efeito (padrão que o script já usa). AppImage: o diretório `usr/share/rustdesk/` e `app_info.id/name` passam a usar `<binario>`. O `bsdtar -zxvf rustdesk.deb` continua, porque o workflow renomeia o `.deb` para esse nome. Escapar o `&` do nome de exibição no lado da substituição do `sed`.

- [ ] **Step 4: Rodar e ver passar**

Run: `bash pcep/testes/aplicar-variante.test.sh`
Expected: termina com código 0 e sem nenhuma linha `FALHOU:`.

- [ ] **Step 5: Workflow: instalar, conferir, remover e publicar o `.deb`**

No job `linux`, depois de mover o `.deb` para `deb/${DEB}` (~:629), adicionar o passo "Testar instalação do .deb":
- `sudo apt-get install -y ./deb/${DEB}`;
- `test -x /usr/share/<binario>/<binario>` e `test -L /usr/bin/<binario>`;
- `systemctl cat <binario>` com sucesso;
- `test -f /usr/share/applications/<binario>.desktop`;
- `dpkg -L <binario> | grep -i rustdesk` sem resultado (falhar se houver);
- `sudo apt-get remove -y <binario>`, depois `test ! -e /usr/bin/<binario>` e `systemctl is-enabled <binario>` falhando.

O serviço pode não subir no runner (sem sessão gráfica). Não exigir `is-active`, só que a unit exista e esteja habilitada antes da remoção (`systemctl is-enabled <binario>` = `enabled`).

Publicar também `deb/${DEB}` como artifact `entregavel-${DEB}`, mantendo `deb-<variante>`, que o job `appimage` consome. No job `release`, trocar a exigência de 6 para 8 arquivos e atualizar o comentário do cabeçalho (`:22-23`) que diz que o `.deb` não é publicado.

- [ ] **Step 6: Commit**

```bash
git add pcep/aplicar-variante.sh pcep/testes/aplicar-variante.test.sh .github/workflows/pcep-build.yml pcep/LEIAME.md
git commit -m "feat(pcep): pacote .deb com nomes próprios por variante"
```

### Task 2: Assinatura do Windows preparada (opcional por segredo)

**Files:**
- Create: `pcep/assinar-windows.ps1`
- Modify: `.github/workflows/pcep-build.yml` (job `windows`: entre o download do topmost ~:380 e o empacotamento portátil ~:382; e depois do `mv` do portátil ~:392)
- Modify: `pcep/LEIAME.md` (seção "Assinatura do Windows")

**Interfaces:**
- Produces: `pcep/assinar-windows.ps1 -Caminho <pasta ou arquivo>`. Lê `PCEP_ASSINATURA_PFX` (base64) e `PCEP_ASSINATURA_SENHA` do ambiente. Sem os dois, imprime `Assinatura: sem certificado, executável não assinado.` e sai com 0. Com os dois, assina com `signtool sign /fd sha256 /td sha256 /tr http://timestamp.digicert.com /f <pfx temporário> /p <senha>` cada `.exe`/`.dll` encontrado (recursivo, ignorando qualquer caminho que contenha `\drivers\` e arquivos cujo `Get-AuthenticodeSignature` já seja `Valid`). Depois confere que cada arquivo assinado tem `SignerCertificate` não nulo. Sai com ≠ 0 em qualquer falha. Apaga o PFX temporário em `finally`. Nunca imprime a senha nem o conteúdo do PFX.
- Opção `-Teste`: gera um certificado autoassinado de code signing (`New-SelfSignedCertificate -Type CodeSigningCert`), exporta para PFX temporário com senha aleatória e assina com ele, sem precisar de segredos. Serve só para validar a mecânica no CI.

- [ ] **Step 1: Escrever o script com o modo `-Teste`**

Localizar o `signtool.exe` no runner pela pasta mais nova de `${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe`. Falhar com mensagem clara se não achar.

- [ ] **Step 2: Ligar no workflow**

Adicionar o input `assinatura_teste` (boolean, padrão `false`) ao `workflow_dispatch`. No job `windows`, adicionar o passo "Assinar arquivos internos" (antes do `generate.py`, com `-Caminho ./rustdesk`) e o passo "Assinar executável portátil" (depois do `mv`, com `-Caminho ./entregaveis/${ENTREGAVEL}`). Os dois recebem `env: PCEP_ASSINATURA_PFX: ${{ secrets.PCEP_ASSINATURA_PFX }}` e `PCEP_ASSINATURA_SENHA: ${{ secrets.PCEP_ASSINATURA_SENHA }}` e passam `-Teste` quando `inputs.assinatura_teste` for verdadeiro. Em tag (release), `-Teste` nunca é usado: condicionar a `github.event_name == 'workflow_dispatch'`.

- [ ] **Step 3: Passo de conferência no modo teste**

Quando `assinatura_teste` for verdadeiro, rodar depois da assinatura do portátil: `(Get-AuthenticodeSignature ./entregaveis/$env:ENTREGAVEL).SignerCertificate` não nulo, senão falha. O status vai ser `UnknownError`/`NotTrusted` por ser autoassinado; isso é esperado. Não usar esse artefato para nada além do teste.

- [ ] **Step 4: Teste de senha errada (Review Focus 3)**

No mesmo passo de conferência do modo teste: chamar o script num arquivo qualquer com `PCEP_ASSINATURA_PFX` = PFX autoassinado e `PCEP_ASSINATURA_SENHA` = `errada`, e exigir código de saída ≠ 0. Gerar o PFX no passo; ele nunca vai para artifact.

- [ ] **Step 5: LEIAME**

Documentar como ligar a assinatura: criar os segredos `PCEP_ASSINATURA_PFX` (`base64 -w0 certificado.pfx`) e `PCEP_ASSINATURA_SENHA` em Settings → Secrets → Actions. Registrar que certificados atuais (Certum na nuvem, Azure Artifact Signing) não exportam PFX: nesse caso só o trecho do `signtool` dentro do script muda (ponto único). Incluir o comando para testar a mecânica: `gh workflow run pcep-build.yml -R DevPCEP/pcep-desk-app -f variante=cliente -f assinatura_teste=true`.

- [ ] **Step 6: Commit**

```bash
git add pcep/assinar-windows.ps1 .github/workflows/pcep-build.yml pcep/LEIAME.md
git commit -m "feat(pcep): assinatura do Windows opcional por segredo"
```

### Task 3: Textos visíveis com o nome de exibição

**Files:**
- Modify: `flutter/lib/common.dart` (getter novo ao lado de `appName`, ~:4272)
- Modify: `flutter/lib/desktop/widgets/update_progress.dart:46,270-271`
- Modify: `flutter/lib/desktop/pages/install_page.dart:214`
- Modify: `flutter/lib/desktop/pages/desktop_setting_page.dart:2450,2470,2485,2495`
- Modify: `flutter/lib/desktop/widgets/tabbar_widget.dart:653-656`
- Modify: `src/tray.rs:87,93`
- Modify: `src/core_main.rs:267,299` (título dos toasts)
- Modify: `src/whiteboard/windows.rs:29`, `src/whiteboard/linux.rs:216`, `src/whiteboard/macos.rs:85`
- Modify: `pcep/aplicar-variante.sh` + teste: `flutter/linux/my_application.cc:145,149` (título literal `"rustdesk"` vira o nome de exibição)

**Interfaces:**
- Consumes: `bind.mainGetAppDisplayNameSync()` (Dart) e `crate::get_app_display_name()` (Rust), ambos do Plano 2.
- Produces: getter Dart `String get appDisplayName => bind.mainGetAppDisplayNameSync();` em `common.dart`.

Regra: troca só o que é texto exibido. Continuam técnicos: `appPrinterName` (`common.dart:4260`, é o nome da impressora instalada e precisa bater com `get_app_name()` no Rust), IDs (`tray.rs:157`, `with_id`), `auth_2fa.rs` (o build PCEP não usa 2FA) e mensagens só de log.

- [ ] **Step 1: Trocar os pontos Dart**

Nos arquivos listados, `{$appName}`/`$appName` dentro de textos exibidos viram `{$appDisplayName}`/`$appDisplayName`. Os textos são chaves de tradução com placeholder (`'Downloading {$appName}'`): o `lang.rs` extrai o valor entre chaves e o devolve no texto, então trocar o valor basta. Em `tabbar_widget.dart`, `Text("RustDesk")` vira `Text(isPcepBuild ? appDisplayName : "RustDesk")`.

- [ ] **Step 2: Trocar os pontos Rust**

`tray.rs` tooltip, títulos de toast em `core_main.rs` e títulos do quadro branco passam a usar `crate::get_app_display_name()`. No quadro branco o título fica `format!("{} - quadro", crate::get_app_display_name())` só em build PCEP (`pcep::embedded().is_some()`); fora dele continua `"RustDesk whiteboard"`.

- [ ] **Step 3: Título GTK do Linux em `aplicar-variante.sh` + teste**

Adicionar `flutter/linux/my_application.cc` a `ALVOS`. Asserção: depois do script, as linhas `gtk_header_bar_set_title` e `gtk_window_set_title` contêm `"<nome de exibição>"` e o arquivo não contém `"rustdesk"`. O `&` em C string não precisa de escape; no `sed`, precisa.

- [ ] **Step 4: Verificar**

Run: `bash pcep/testes/aplicar-variante.test.sh` → código 0, nenhum `FALHOU:`.
Run: `cargo test -p pcep --lib` → `test result: ok. 25 passed`.
Run: `cargo check` não é viável localmente (dependências nativas do upstream). A compilação de Rust/Dart é validada no CI. Por isso: `grep -n "appDisplayName" flutter/lib/common.dart` acha o getter; `grep -rn '{\$appName}' flutter/lib/desktop` só pode sobrar em pontos não visíveis, cada um justificado no relatório.

- [ ] **Step 5: Commit**

```bash
git add -A flutter/lib src/tray.rs src/core_main.rs src/whiteboard pcep/aplicar-variante.sh pcep/testes/aplicar-variante.test.sh
git commit -m "fix(pcep): textos visíveis com o nome de exibição"
```

### Task 4: Título da janela principal no Windows

**Files:**
- Modify: `src/flutter.rs:228-229` (`get_rustdesk_app_name`)
- Modify: `src/core_main.rs:821-827` (`send_message_to_hnwd`)
- Modify: `flutter/lib/common.dart:3099-3108` (`getWindowName`)

**Interfaces:**
- Consumes: `crate::get_app_display_name()`.
- Produces: no build PCEP, o título da janela principal no Windows é o nome de exibição, e as três pontas que localizam a janela pelo título (`main.cpp:97` via `get_rustdesk_app_name`, `core_main.rs:824` e o título definido pelo Dart) usam o mesmo valor.

`main.cpp` não muda: ele recebe o nome de `get_rustdesk_app_name`, que passa a exportar `get_app_display_name()`. Com isso, os títulos "Connection Manager" e "Install" também mostram o nome de exibição. O `core_main.rs:824` passa a `&crate::get_app_display_name()`. `getWindowName` para `WindowType.Main` retorna `name` em todas as plataformas e o comentário `PCEP:` é atualizado. Fora do build PCEP, `get_app_display_name()` devolve `get_app_name()`, então o comportamento do upstream não muda.

- [ ] **Step 1: Confirmar que não há outro usuário do título técnico**

Run: `grep -rn "get_rustdesk_app_name\|FLUTTER_RUNNER_WIN32_WINDOW_CLASS" src flutter/windows`
Expected: só `flutter.rs`, `main.cpp`, `core_main.rs` e a definição da classe em `platform/windows.rs`. Se aparecer outro uso, ele passa a usar o nome de exibição também, ou fica registrado no relatório o motivo para não usar.

- [ ] **Step 2: Aplicar as três trocas**

- [ ] **Step 3: Roteiro manual**

Adicionar ao `docs/superpowers/plans/2026-11-04-validacao-e2e.md` (repo do painel) a Parte D com:
- D.1: `.deb` do cliente instala no Linux, aparece no menu como "Pc&P Desk", o serviço `pcepdesk` fica ativo e o computador aparece no painel.
- D.2: `.deb` do colaborador instala e coexiste com o do cliente e com o RustDesk original.
- D.3: `sudo apt remove pcepdesk` remove o serviço e o atalho.
- D.4: Bandeja, tela de atualização e barra de abas mostram "Pc&P Desk".
- D.5: No Windows, a barra de tarefas e o Alt+Tab mostram "Pc&P Desk" (e "Pc&P Desk - Connection Manager" na janela de aceite).
- D.6: Com o app aberto no Windows, abrir o app de novo traz a janela existente em vez de abrir outra.

- [ ] **Step 4: Commit (dois repositórios)**

```bash
git add src/flutter.rs src/core_main.rs flutter/lib/common.dart
git commit -m "fix(pcep): título da janela do Windows com o nome de exibição"
# repo do painel:
git add docs/superpowers/plans/2026-11-04-validacao-e2e.md
git commit -m "docs: roteiro — Parte D (acabamento)"
```

### Task 5: Links dos `.deb` na página de download (painel)

**Files:**
- Modify: `painel/src/web/downloads.rs`
- Modify: `painel/templates/baixar.html`
- Test: `painel/tests/web_downloads.rs`

**Interfaces:**
- Consumes: nomes de arquivo da Task 1: `PcepDesk-Linux-x86_64.deb`, `PcepDeskEquipe-Linux-x86_64.deb`.

- [ ] **Step 1: Teste que falha**

Em `download_links_point_to_latest_release`, incluir os dois `.deb` na lista de nomes. Em `download_page_shows_instructions_and_brand`, exigir `body.contains("Linux (.deb)")` e `body.contains("sudo apt install")`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cd painel && cargo test --test web_downloads`
Expected: FAIL `link ausente: PcepDesk-Linux-x86_64.deb`.

- [ ] **Step 3: Implementar**

Adicionar os dois entregáveis com o rótulo "Linux (.deb)" e a instrução: "Ubuntu/Debian: baixe e rode `sudo apt install ./PcepDesk-Linux-x86_64.deb` (instala e inicia junto com o computador). Para usar sem instalar, prefira o AppImage." (para a equipe, troque o nome do arquivo).

- [ ] **Step 4: Rodar a suíte inteira**

Run: `cd painel && cargo test`
Expected: todos passando (181 + os novos asserts; nenhum `FAILED`).

- [ ] **Step 5: Commit**

```bash
git add painel/src/web/downloads.rs painel/templates/baixar.html painel/tests/web_downloads.rs
git commit -m "feat(web): links dos pacotes .deb na página de download"
```

### Fechamento

- [ ] Revisão final pelo `auditor`: segredos de assinatura, scripts de instalação como root no `.deb`. Escopo: diff do fork desde `9cfcce31f` e do painel desde `4f1b21e`.
- [ ] O usuário faz o push dos dois repositórios e roda `gh workflow run pcep-build.yml -R DevPCEP/pcep-desk-app -f variante=ambas -f assinatura_teste=true`. Esperado: todos os jobs com sucesso, 8 entregáveis e passos de assinatura de teste com sucesso.
- [ ] O usuário roda a Parte D do roteiro.
