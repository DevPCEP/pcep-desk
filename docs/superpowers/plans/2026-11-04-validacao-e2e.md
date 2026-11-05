# Validação ponta a ponta do PCEP Desk (servidor + painel)

Este documento tem duas partes:

- **Parte A:** o que já foi verificado automaticamente (smoke test), com resultado e data.
- **Parte B:** o roteiro manual para você seguir com **duas máquinas reais** e o **RustDesk original**. Ele confirma o que um script não consegue ver: o comportamento do aplicativo de verdade (janela de aceite, desligamento da sessão, app portátil).

Marque cada caixa `- [ ]` ao concluir; se algo sair diferente do esperado, escreva na linha "Anote" do item.

## Versões usadas

| Componente | Versão |
|---|---|
| Servidor de conexão (`hbbs` e `hbbr`) | rustdesk-server **1.1.16** (release oficial, binários `linux-amd64`) |
| Painel | commit `325dd33` (branch `plano-1-servidor-painel`) |
| Rust (compilação do painel) | 1.95.0 |
| RustDesk original (app) | **preencha:** versão exata das duas máquinas (Ajuda → Sobre) |
| Sistema da máquina do cliente | **preencha** (ex.: Windows 11) |
| Sistema da máquina da colaboradora | **preencha** |

## Parte A: smoke test automatizado

**Data:** 04/11/2026. **Script:** `servidor/smoke-test.sh` (bash, sem Docker). **Resultado: 30 verificações, 0 falhas** (código de saída 0).

O que o script faz: baixa o `rustdesk-server` 1.1.16 para uma pasta temporária, sobe `hbbs -k _` e `hbbr -k _`, compila e sobe o painel com banco temporário em `http://127.0.0.1:18080` e simula com `curl` o que o app RustDesk faz conforme o contrato da API. Ao sair, encerra tudo e apaga os temporários. Para repetir:

```bash
cd pcep-desk
servidor/smoke-test.sh
```

(Precisa de `curl`, `unzip`, `jq`, `ss`, `cargo`, internet na primeira vez e das portas 21115 a 21119 e 18080 livres; se estiverem ocupadas o script avisa e para.)

O que foi confirmado:

- `hbbs` gerou `id_ed25519.pub` e `hbbs`/`hbbr` ficaram escutando (21116 e 21117/tcp).
- Painel: administrador criado pela linha de comando; login web por cookie; colaboradora criada pelo painel.
- Computador do "cliente": `heartbeat` (corpo `{}`) e `sysinfo` (`SYSINFO_UPDATED`); aparece em **Não vinculados**; cliente criado; computador vinculado (código digitado com espaços, `111 222 333`); some de Não vinculados.
- App da colaboradora: login (`/api/login` com o código do computador), nome de exibição `Maria – PCEP`, `currentUser` 200, `/api/ab/personal`, `/api/ab/shared/profiles` (catálogo `Clientes PCEP`, `clientes-pcep`, somente leitura), `/api/ab/peers?ab=clientes-pcep` com o alias `Padaria Smoke – Caixa`.
- Histórico: eventos `new`, `authorized` (com `peer = [código da colaboradora, nome]`) e `close` → `/historico` mostra **Conectado** com o nome da colaboradora; tentativa só com `new` + `close` → **Não concluída**.
- Desativar a colaboradora: `currentUser` com o token antigo → **401**; novo login → recusado com "Usuário desativado".

Saída real (resumo):

```
OK hbbs gerou id_ed25519.pub
OK login web do administrador (303, cookie de sessão)
OK computador do cliente aparece em Não vinculados
OK ab/peers do catálogo compartilhado traz o alias "Padaria Smoke – Caixa"
OK histórico mostra "Conectado" com o nome da colaboradora
OK histórico mostra "Não concluída" para a tentativa recusada
OK currentUser com o token antigo responde 401 após desativar
== Resumo: 30 verificações, 0 falha(s)
```

**O que o smoke test NÃO cobre** (é o objetivo da Parte B): o comportamento real do app RustDesk (nome na janela de aceite, tempo até aparecer em Não vinculados, se o app portátil envia heartbeat, se o app realmente desloga depois do 401), a conexão de fato entre duas máquinas (NAT, relay) e o HTTPS com Caddy.

---

## Parte B: roteiro manual com duas máquinas

### B.0 Preparação

Você precisa de:

- **Servidor:** uma máquina Linux com o sistema no ar, seguindo `servidor/README.md` (Docker Compose), **ou** só para teste, `hbbs`/`hbbr` e painel rodando como no smoke test (`PAINEL_URL_PUBLICA` com o IP/endereço que as duas máquinas alcançam, por exemplo `http://192.168.0.10:8080`, e `PAINEL_COOKIE_SEGURO=false` se não houver HTTPS).
- **Máquina "colaboradora":** RustDesk **original instalado** (pode ser a sua).
- **Máquina "cliente":** RustDesk **original**. Para o item de app portátil, use o executável **sem instalar** (no Windows, o `rustdesk-*.exe` aberto direto, sem clicar em "Instalar").
- No painel (`PAINEL_URL_PUBLICA`): um administrador (`painel criar-admin ...`) e, em **Colaboradores**, uma colaboradora de teste com nome de exibição **`Maria – PCEP`** (usuário `maria`).

**Configuração do RustDesk original nas DUAS máquinas** (Configurações → Rede; clique no cadeado para desbloquear):

| Campo | Valor |
|---|---|
| Servidor de ID | endereço do servidor (IP ou domínio), por exemplo `192.168.0.10` |
| Servidor de relay | o mesmo endereço |
| Servidor de API | `http://<endereço>:8080` ou `https://<domínio>` (igual à `PAINEL_URL_PUBLICA`) |
| Chave | conteúdo do arquivo `id_ed25519.pub` (em `servidor/dados/rustdesk/` no Docker) |

Clique em **OK**. O indicador na tela inicial deve ficar verde ("Pronto"). Anote o **código de conexão** (ID) de cada máquina.

- [ ] **B.0.1 As duas máquinas ficam "Pronto" (verde).**
  - Fazer: configurar como na tabela acima e conferir o indicador.
  - Deve acontecer: indicador verde nas duas, com código de conexão exibido.
  - Anote se diferente: mensagem do indicador (ex.: "Serviço não está rodando", "ID/relay inválido"), firewall/portas 21115 a 21119 e se a chave foi colada inteira.

### B.1 Login no app da colaboradora

- [ ] **B.1.1 Login mostra o nome de exibição.**
  - Fazer: no app da colaboradora, clique no ícone de conta (ou "Entrar") e entre com `maria` e a senha cadastrada.
  - Deve acontecer: login concluído e o nome **Maria – PCEP** aparece no app (menu da conta).
  - Anote se diferente: mensagem de erro exata; se entrou mas mostra só `maria` (ou outro nome); se pediu algum método de login diferente (OIDC, 2FA).

### B.2 Cliente sem instalar aparece em "Não vinculados"

- [ ] **B.2.1 Cliente rodando SEM instalar aparece em Não vinculados em até ~30 s.**
  - Fazer: na máquina do cliente, abra o RustDesk **sem instalar** e deixe aberto. No painel (logado), abra **Não vinculados** e atualize por até 1 minuto.
  - Deve acontecer: em até cerca de 30 segundos aparece uma linha com o código da máquina, nome do computador (hostname), sistema e "Online".
  - Anote se diferente: quanto tempo levou; se não apareceu nada (ver também B.8); se apareceu sem nome do computador/sistema (indica que só o `heartbeat` chegou, não o `sysinfo`).
- [ ] **B.2.2 Vincular a um cliente.**
  - Fazer: crie o cliente "Cliente de Teste" em **Clientes**; em **Não vinculados**, clique em **Vincular**, escolha o cliente, rótulo "Computador" e salve.
  - Deve acontecer: a máquina some de Não vinculados e aparece no detalhe do cliente com o código e o estado "Online".
  - Anote se diferente: mensagem de erro; se o código aparece diferente do mostrado no app.

### B.3 Catálogo "Clientes PCEP" no app

- [ ] **B.3.1 O catálogo compartilhado aparece e mostra "Cliente de Teste – Computador".**
  - Fazer: no app da colaboradora (logada), abra a aba de catálogo/endereços e escolha o catálogo **Clientes PCEP**. Se não aparecer, feche e abra o app, ou use o botão de atualizar.
  - Deve acontecer: o catálogo **Clientes PCEP** existe (somente leitura) e traz o item **Cliente de Teste – Computador** (nome do cliente, travessão, rótulo).
  - Anote se diferente: se o catálogo some, aparece vazio, ou se só há o "Catálogo pessoal"; se permite editar/apagar itens do compartilhado; texto de qualquer erro.
- [ ] **B.3.2 Busca pelo nome do cliente funciona.**
  - Fazer: no campo de busca do catálogo, digite parte do nome do cliente (ex.: `teste`).
  - Deve acontecer: o item "Cliente de Teste – Computador" é listado.
  - Anote se diferente: se a busca só acha pelo código e não pelo nome.

### B.4 Conectar e a janela de aceite

- [ ] **B.4.1 A janela de aceite no cliente mostra "Maria – PCEP".**
  - Fazer: no app da colaboradora, conecte ao item "Cliente de Teste – Computador" (clique duas vezes). Olhe a janela de pedido de acesso na máquina do cliente.
  - Deve acontecer: a janela mostra o nome **Maria – PCEP** como quem pede acesso.
  - Anote se diferente: o texto exato mostrado (por exemplo o código `987654321`, ou o nome do usuário do sistema operacional). Isso decide se o fork (Plano 2) precisa forçar o nome; registre mesmo assim, não é falha do servidor.
- [ ] **B.4.2 Aceitar, usar e encerrar gera "Conectado" com duração.**
  - Fazer: na máquina do cliente, clique em **Aceitar**. Na colaboradora, use a sessão por cerca de 1 minuto e depois encerre (fechar a janela remota). Abra **Histórico** no painel.
  - Deve acontecer: aparece uma linha com situação **Conectado**, colaboradora **Maria – PCEP**, o computador/cliente certo, a duração (aprox. 1 min), tipo "Acesso remoto" e o IP.
  - Anote se diferente: se aparece mais de uma linha para a mesma conexão (duplicata), colaboradora em branco, duração zerada, cliente não identificado; horário (deve estar em UTC−03:00).
- [ ] **B.4.3 Recusar uma tentativa gera "Não concluída".**
  - Fazer: conecte de novo pela colaboradora e, na máquina do cliente, clique em **Recusar** (ou deixe expirar sem aceitar). Abra **Histórico**.
  - Deve acontecer: uma nova linha com situação **Não concluída**, sem duração.
  - Anote se diferente: se não aparece nenhuma linha (o app pode não enviar o evento de tentativa recusada, o que o Plano 2 resolveria); se aparece como "Conectado".

### B.5 Desativar a colaboradora

- [ ] **B.5.1 O app desloga na próxima verificação.**
  - Fazer: como administrador, em **Colaboradores**, **Desativar** a `maria`. No app da colaboradora, aguarde até alguns minutos, ou feche e abra o app, ou abra o menu da conta.
  - Deve acontecer: o app volta ao estado "não logado" (o servidor passa a responder 401 em `/api/currentUser`); tentar entrar de novo mostra "Usuário desativado".
  - Anote se diferente: quanto tempo levou; se o app continuou logado mesmo depois de reiniciar; se o catálogo "Clientes PCEP" continuou aparecendo.
- [ ] **B.5.2 Reativar devolve o acesso (opcional).**
  - Fazer: **Reativar** a colaboradora e entrar de novo no app.
  - Deve acontecer: login normal.
  - Anote se diferente: mensagem de erro.

### B.6 Painel por navegador (sanidade)

- [ ] **B.6.1 Login web, busca e histórico.**
  - Fazer: entre no painel pelo navegador com `maria` (reativada) e com o administrador; busque o cliente por parte do nome na lista; abra **Histórico** filtrando por cliente e por data.
  - Deve acontecer: colaboradora vê só o próprio histórico e não vê **Colaboradores**; administrador vê tudo; busca e filtros respondem.
  - Anote se diferente: página que deu erro e a mensagem.

### B.7 Estado "Online/Offline" do cliente

- [ ] **B.7.1 O estado do computador acompanha o app.**
  - Fazer: feche o RustDesk na máquina do cliente e espere cerca de 1 minuto; abra de novo.
  - Deve acontecer: no detalhe do cliente o computador passa a "Offline" e depois volta a "Online" depois de abrir o app.
  - Anote se diferente: tempos observados.

### B.8 O app portátil envia heartbeat?

Este é o ponto em aberto do projeto: o painel só descobre computadores que enviam `heartbeat`/`sysinfo`. Se o app portátil (sem instalar) não enviar, ele **não** aparece em Não vinculados e o computador teria de ser cadastrado à mão pelo código.

- [ ] **B.8.1 Portátil sem instalar.** (é o mesmo teste de B.2.1; registre o resultado aqui)
  - Fazer: com o RustDesk **sem instalar** aberto na máquina do cliente, aguarde 2 minutos e olhe **Não vinculados**.
  - Deve acontecer: a máquina aparece (heartbeat e sysinfo recebidos).
  - Anote: **SIM** ou **NÃO** apareceu; em caso de NÃO, repita com o RustDesk **instalado** (serviço rodando) e anote se apareceu. Se só aparece instalado, o app portátil não envia heartbeat; a alternativa é cadastrar o código pelo botão de novo computador no cliente.
  - Resultado registrado: **preencha**: portátil envia heartbeat? ( ) sim ( ) não; instalado envia? ( ) sim ( ) não.

### B.9 Fechamento

- [ ] **B.9.1 Divergências do contrato registradas.** Anote abaixo qualquer comportamento do app que fugiu do esperado (itens acima com "Anote se diferente" preenchidos) para virarem correções do painel ou requisitos do Plano 2.

**Divergências encontradas:**

```
(preencha)
```

**Conclusão do teste manual:** ( ) tudo conforme ( ) com divergências (listadas acima)
**Data e quem testou:** ____

## Parte C: apps próprios Pc&P Desk (Plano 2)

Mesmo ambiente da Parte B (servidor `hbbs`/`hbbr` + painel na máquina Linux; VM Windows em modo bridge). Os apps já vêm com servidor, chave e API gravados — **não configure nada na tela de rede**.

**Onde baixar (antes da primeira release):** página da execução do workflow `pcep-build.yml` no GitHub → seção *Artifacts* → `entregavel-PcepDesk-Windows-x64.exe` (cliente, para a VM) e `entregavel-PcepDeskEquipe-Linux-x86_64.AppImage` (equipe, para a máquina Linux). Os artifacts vêm em `.zip`; descompacte.
**Depois da release:** `http://<IP>:8080/baixar`.

Versões: commit do app `9cfcce31f`, execução do workflow `42012308636`.

- [x] **C.1 Cliente abre sem instalar e sem configurar**
  - Fazer: na VM, rodar `PcepDesk-Windows-x64.exe` (se o Windows avisar "protegeu o computador": Mais informações → Executar assim mesmo).
  - Deve acontecer: janela "Pc&P Desk" com ícone do escritório, código de conexão visível, sem tela para trocar servidor; botão "Instalar" visível.
  - Anote se diferente: conforme.
- [x] **C.2 Cliente aparece no painel**
  - Fazer: no painel, abrir "Não vinculados".
  - Deve acontecer: o computador da VM aparece em até ~30 s. Vincular a um cliente.
- [x] **C.3 Configurações bloqueadas no cliente**
  - Fazer: procurar Configurações/servidor/senha permanente no app do cliente.
  - Deve acontecer: não há como mudar servidor, senha permanente ou modo de aceite.
- [x] **C.4 App da equipe exige login**
  - Fazer: na máquina Linux, dar permissão de execução e abrir `PcepDeskEquipe-Linux-x86_64.AppImage`; SEM login, tentar conectar ao código da VM.
  - Deve acontecer: aviso "Faça login no Pc&P Desk Equipe para conectar." e nenhuma conexão.
- [x] **C.5 Link sem login também bloqueado**
  - Fazer: sem login, rodar `./PcepDeskEquipe-Linux-x86_64.AppImage --connect <código da VM>`.
  - Deve acontecer: janela com mensagem de erro, sem conexão.
- [x] **C.6 Com login: catálogo e aceite**
  - Fazer: entrar com a colaboradora do painel; abrir o catálogo "Clientes PCEP"; conectar ao computador do cliente.
  - Deve acontecer: na VM aparece a janela de aceite com o nome da colaboradora (ex.: "Maria – PCEP"); só conecta depois do clique em Aceitar. Não há opção de entrar com senha.
- [x] **C.7 Histórico**
  - Fazer: aceitar, usar, encerrar; depois recusar uma tentativa.
  - Deve acontecer: no painel, "Conectado" com duração e a colaboradora; a recusa como "Não concluída".
- [x] **C.8 Cliente com a janela fechada**
  - Fazer: na VM, instalar o Pc&P Desk (botão Instalar), fechar a janela principal (deixando o serviço) e tentar conectar.
  - Deve acontecer: conexão recusada.
- [x] **C.9 RustDesk original não interfere**
  - Fazer: na mesma VM, se houver RustDesk original com senha permanente, tentar entrar no Pc&P Desk com essa senha.
  - Deve acontecer: não entra; o Pc&P Desk só aceita por clique.
- [x] **C.10 Sair do login derruba a reconexão**
  - Fazer: conectado, sair do login no app da equipe e tentar reconectar.
  - Deve acontecer: reconexão bloqueada com mensagem.
- [x] **C.11 Sobre e nomes**
  - Fazer: abrir Configurações → Sobre no app da equipe.
  - Deve acontecer: "Pc&P Desk — baseado no RustDesk (AGPL-3.0)" e link do código-fonte; sem links para rustdesk.com (exceto o crédito "powered by").
- [x] **C.12 Executável renomeado não troca o servidor (Windows)**
  - Fazer: copiar `PcepDesk-Windows-x64.exe` com o nome `PcepDesk-host=1.2.3.4,key=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=.exe` e abrir.
  - Deve acontecer: continua aparecendo no painel do escritório (não usa 1.2.3.4).
- [x] **C.13 Colaborador desativado é barrado sem reiniciar o app**
  - Fazer: com o app da equipe aberto e logado, desativar a colaboradora no painel; tentar conectar.
  - Deve acontecer: aviso "Faça login no Pc&P Desk Equipe para conectar." e nenhuma conexão.

**Resultado da Parte C (2026-11-05):** todos os itens conforme, testados com o app da equipe no Linux e o cliente em um PC Windows 10 físico.
- C.8: testado com o app sem instalar; ao fechar a janela, o app encerra e a conexão é recusada (esperado).
- Divergência de ambiente (pendência, não bloqueia): na VM Windows, depois do Aceitar, a janela do cliente trava (só aparece na barra de tarefas) e o colaborador fica em "aguardando aceite"; o Windows fecha a conexão ~1min30 depois. No PC físico funciona. Suspeita: VM sem aceleração de vídeo ou regressão do RustDesk 1.5.x em VM; falta o log da VM.
