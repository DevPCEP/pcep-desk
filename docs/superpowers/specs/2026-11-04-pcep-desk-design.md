# PCEP Desk — Desenho

Data: 2026-11-04
Status: rascunho para aprovação (revisado após checagem do construtor)

## 1. Intenção e motivo

O escritório (PCEP, contabilidade) atende clientes por acesso remoto o tempo todo. Hoje usa AnyDesk gratuito (limitado em volume de acessos) e TeamViewer pago (poucas licenças). O problema principal é **limite de conexões e de permissões** desses produtos.

O PCEP Desk é uma versão própria do RustDesk (open-source, AGPL-3.0), com servidor próprio e um painel simples de gestão, sem custo de licença. Substitui AnyDesk/TeamViewer no dia a dia do escritório.

Sucesso significa: qualquer colaborador consegue atender qualquer cliente, sem limite de licença; o cliente sabe sempre quem está entrando; o escritório sabe quem atendeu quem; e ninguém precisa perguntar o código de conexão a um cliente já cadastrado.

## 2. Objetivos

1. **Servidor próprio** de conexão (`hbbs`/`hbbr` do `rustdesk-server` OSS) — sem limite de conexões, sem depender do servidor público da RustDesk.
2. **App do cliente** ("QS"): executável que roda sem instalar; instala só se o cliente quiser. Só recebe conexão.
3. **App do colaborador**: app completo, com login no painel, base de clientes e conexão.
4. **Painel web** com:
   - **Colaboradores**: cadastro de usuários do escritório. O nome cadastrado (ex.: "Maria – PCEP") é o que aparece para o cliente na hora da conexão.
   - **Histórico**: quem conectou em qual cliente, quando, por quanto tempo; e tentativas que não se concretizaram (recusadas/falhas), na medida em que o app reportar.
   - **Base de clientes**: cliente → computadores → código de conexão (ID RustDesk). Pesquisável pelo painel e visível dentro do app do colaborador (catálogo de endereços compartilhado), com clique para conectar.
5. **Multiplataforma**: Windows (maioria), Linux e macOS — tanto no app do cliente quanto no do colaborador.

## 3. Não-objetivos (fora desta etapa)

- Acesso sem supervisão (entrar sem ninguém do outro lado).
- Cobrança, chamados/tickets, relatórios de produtividade.
- Restringir quais clientes cada colaborador vê (todos veem a base inteira).
- Android/iOS.
- Login via Google/Microsoft (OIDC). Só usuário e senha.

## 4. Anti-objetivos (seria fracasso mesmo funcionando)

- **O escritório conseguir entrar num cliente sem o cliente aceitar.** Isso destrói a confiança e cria suspeita. A regra de aceitação manual é inegociável no app do cliente e não pode ser desligada nem pelo cliente nem pelo técnico.
- Cliente ver um número ou o nome do computador do técnico em vez do nome do colaborador.
- Depender de qualquer serviço pago ou do servidor público da RustDesk para funcionar.
- Usar o nome ou a logo "RustDesk" (marca registrada).
- Distribuir o executável sem disponibilizar o código-fonte modificado (viola a AGPL).

## 5. Restrições

- **Licença AGPL-3.0**: o código modificado do app fica num repositório público (ex.: GitHub), com link acessível a quem recebe o executável. O painel é obra própria e pode ter a licença que o escritório escolher; recomenda-se manter no mesmo repositório público por simplicidade.
- **Marca**: nome de exibição **"Pc&P Desk"** (decisão do usuário, 2026-11-04) — usado em títulos de janela, textos da interface e no painel. Nome técnico **`pcep-desk`** / `PcepDesk` (sem "&") para nome do executável, pacotes, identificador de app no macOS, esquema de URI e caminhos de arquivo, porque "&" quebra nomes de arquivo, scripts e XML. Ícone: ainda não existe; o usuário criará um a partir do logo do escritório. Até lá, ícone provisório, com todos os ícones reunidos num só lugar para a troca ser simples.
- **Configuração gravada no código**: o mecanismo oficial de cliente personalizado (`custom.txt`) exige assinatura com a chave da RustDesk (`src/common.rs:2593`, só no plano pago). Portanto a configuração (servidor, chave pública, servidor de API, nome do app, modo do app, regras de segurança) é definida no código-fonte antes de compilar. Pontos de partida conhecidos: `libs/hbb_common/src/config.rs:116-117` (`RENDEZVOUS_SERVERS`, `RS_PUB_KEY`), `config::HARD_SETTINGS`/`OVERWRITE_SETTINGS`, `is_incoming_only` e `is_disable_installation` (`src/flutter_ffi.rs:2555,2584`).
- **Infraestrutura indefinida**: pode ser um servidor no escritório (exige IP fixo/DDNS e portas liberadas) ou uma VPS básica (Hostinger). Tudo do lado servidor roda em **Docker Compose**, igual nos dois cenários.
- **Mantenedor**: o próprio usuário, que está aprendendo e quer uma stack só. Código legível e bem comentado vale mais que esperteza.
- **Escala**: escritório pequeno/médio — dezenas de colaboradores, centenas de clientes/computadores.

## 6. Decisões já tomadas

### 6.1 Fluxo de atendimento
1. Cliente pede ajuda.
2. Colaborador abre o app do colaborador (logado), pesquisa o cliente na base, vê os computadores e clica para conectar. Se o cliente for novo, pede o código uma vez e cadastra.
3. Cliente, com o app do cliente aberto, vê a janela de pedido de conexão com o nome do colaborador e **aceita com um clique**.
4. A conexão é registrada no histórico.

### 6.2 App do cliente
- Roda sem instalar; a instalação é opcional mas **visível**: o cartão "Instalar" que o RustDesk já mostra no Windows quando o app não está instalado (`flutter/lib/desktop/pages/desktop_home_page.dart:461-467`) permanece. Portanto **não** ativar `is_disable_installation`. Confirmar que o cartão aparece no layout do modo "incoming only" (que oculta o painel direito, linhas 61-68); se não aparecer, trazê-lo para o painel visível.
- Motivo da instalação ficar à mão: sem instalar não há serviço, então o app não interage com janelas de elevação (UAC), tela de bloqueio/login, nem programas rodando como administrador — comum em atendimentos de contabilidade (certificado digital, token, programas da Receita). Nesses casos o colaborador orienta o cliente a clicar em "Instalar". Mesmo instalado, a regra de aceite manual continua valendo.
- Só recebe conexões (modo "incoming only").
- Aprovação manual obrigatória por clique; sem senha permanente; sem acesso sem supervisão. Opções travadas (não editáveis na interface).
- Aponta fixo para o servidor do escritório e para a API do painel (para reportar histórico e status online).
- O código (ID) precisa se manter o mesmo a cada abertura no mesmo computador, para a base de clientes funcionar.

### 6.3 App do colaborador
- App completo do RustDesk rebatizado, apontando fixo para o servidor e para a API do painel.
- **Só conecta com login feito** (decisão do desenho): sem login, o RustDesk enviaria o usuário do sistema operacional (`src/client.rs:3820-3821`), o que viola o anti-objetivo. O botão de conectar fica bloqueado até haver login. O login permanece salvo, então uma queda breve do painel não impede atendimentos com IDs já conhecidos.
- Login com usuário do painel; o nome de exibição da conta é enviado ao cliente na conexão (confirmado: `src/client.rs:3805-3822` usa `display_name` do `user_info` da conta logada; sem login, cai no usuário do sistema operacional).
- Catálogo de endereços compartilhado = base de clientes do painel.

### 6.4 Painel
- **Linguagem**: Rust (mesma do RustDesk — uma linguagem só para aprender e manter).
- **Web**: Axum.
- **Banco**: SQLite (um arquivo; backup é copiar o arquivo).
- **Interface**: HTML renderizado no servidor (templates) com htmx para interatividade. Sem SPA, sem ecossistema JavaScript separado.
- O painel implementa a API HTTP que o app RustDesk chama (`/api/login`, `/api/logout`, `/api/currentUser`, `/api/login-options`, `/api/heartbeat`, `/api/sysinfo`/`sysinfo_ver`, `/api/audit/{conn,file,alarm}`, `/api/ab*`, `/api/peers`, `/api/users`, conforme o necessário). O projeto `lejianwen/rustdesk-api` (MIT, Go, parado desde set/2025) serve de **referência** do contrato, não de dependência.
- Papéis: **administrador** (gerencia colaboradores, vê tudo) e **colaborador** (usa a base de clientes e conecta; vê o próprio histórico).

### 6.5 Servidor
- Docker Compose com: `hbbs` + `hbbr` (rustdesk-server OSS), painel, e proxy com HTTPS automático (para a API e o painel).
- Chave do servidor gerada uma vez; a chave pública é embutida nos apps na compilação.

### 6.6 Compilação
- Fork do RustDesk no GitHub, compilado pelos workflows existentes (`.github/workflows/flutter-build.yml`) adaptados, gerando artefatos para Windows, Linux e macOS, nas duas variantes (cliente e colaborador).

### 6.7 Decisões complementares (tomadas no desenho, a conferir pelo usuário)
- **Autenticidade do nome do colaborador: informativa nesta etapa** (decisão do usuário). O nome é enviado por quem conecta e não é verificado pelo app do cliente; protege contra engano, não contra má-fé. Mitigações obrigatórias: `hbbs`/`hbbr` exigindo a chave do servidor (`-k _`), e desativação de conta no painel invalidando os tokens do colaborador imediatamente. Validação criptográfica do nome no app do cliente fica como evolução futura.
- **Endereço**: subdomínio do domínio do escritório (ex.: `desk.pcepcontabil.com.br`) apontando para o servidor, seja VPS ou servidor local (neste caso com IP fixo ou DDNS + CNAME). O nome é gravado nos apps; trocar de máquina é só mudar o DNS.
- **Distribuição e atualização**: executáveis publicados em GitHub Releases do fork (que também cumpre a exigência de código-fonte da AGPL); o painel tem uma página de download com os links e o link do código-fonte. A verificação de atualização que aponta para o upstream é desligada ou redirecionada para os releases do fork.
- **ID por computador**: comportamento do upstream mantido. Usuários diferentes do Windows no mesmo PC, ou portátil vs. instalado, podem ter IDs diferentes; cada ID é um "computador" na base. Sem derivar ID do hardware.
- **Computadores desconhecidos**: IDs que aparecem no histórico ou no heartbeat sem vínculo ficam numa lista "não vinculados", com ação "vincular a um cliente". O cadastro é sempre confirmado por uma pessoa.
- **Tentativas recusadas** (adiado pelo usuário em 2026-11-04: o app do colaborador NÃO reporta tentativas nesta etapa; o histórico mostra "Não concluída" pelo lado do cliente, sem o nome de quem tentou): o registro de entrada do lado do cliente sai antes do login e só traz IP (`src/server/connection.rs:1527-1534`). Para registrar "quem tentou e não conseguiu", o app do colaborador também reporta suas tentativas ao painel.

## 7. Pontos de atenção conhecidos

- **macOS**: o app precisa de permissões de Gravação de Tela e Acessibilidade, concedidas manualmente pelo cliente na primeira vez. Sem assinatura e notarização Apple (conta de desenvolvedor, paga anualmente), o macOS bloqueia a abertura e exige contornar o aviso de segurança. Decidir se vale pagar.
- **Windows**: executável sem certificado de assinatura de código dispara o aviso do SmartScreen ("O Windows protegeu o computador"). Funciona clicando em "Mais informações → Executar assim mesmo". Certificado é pago; decidir depois.
- **Linux com Wayland**: captura de tela e controle têm limitações conhecidas no RustDesk; X11 funciona melhor.
- **Histórico**: o registro de conexão (`/api/audit/conn`) é enviado pelo lado que **recebe** a conexão (`src/server/connection.rs:1542,1563`), ou seja, pelo app do cliente. Por isso o app do cliente precisa apontar para a API. Tentativas recusadas podem não gerar registro desse lado; verificar e, se preciso, registrar também a partir do app do colaborador.
- **Atualizações do RustDesk**: o fork precisa ser atualizado periodicamente a partir do original. Manter as mudanças no app pequenas e concentradas para facilitar isso.

## 8. Deixado para quem for construir

- Estrutura de pastas e divisão em fases/marcos.
- Esquema do banco e formato exato de cada endpoint (seguindo o que o app espera).
- Como travar as configurações de segurança no app do cliente (constantes, `HARD_SETTINGS`/`OVERWRITE_SETTINGS`, remoção de elementos da interface).
- Ícone, cores e textos da interface.
- Escolha do proxy HTTPS (Caddy é o padrão sugerido).
- Testes: endpoints da API com testes automatizados; apps verificados manualmente nas três plataformas.
