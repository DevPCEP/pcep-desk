// Pc&P Desk: botão "Copiar" do código de conexão, filtro simples de select,
// botão de tema e barra lateral recolhível.
(function () {
  "use strict";

  // Botão copiar: <button type="button" data-copiar="CODIGO">.
  document.addEventListener("click", function (ev) {
    var botao = ev.target.closest ? ev.target.closest("[data-copiar]") : null;
    if (!botao) return;
    var original = botao.dataset.original || botao.textContent;
    botao.dataset.original = original;
    var avisar = function (texto) {
      botao.textContent = texto;
      clearTimeout(botao._timer);
      botao._timer = setTimeout(function () {
        botao.textContent = original;
      }, 2000);
    };
    if (!navigator.clipboard) {
      avisar("Não foi possível copiar");
      return;
    }
    navigator.clipboard.writeText(botao.dataset.copiar).then(
      function () { avisar("Copiado!"); },
      function () { avisar("Não foi possível copiar"); }
    );
  });

  // Filtro de select: <input data-filtrar="ID_DO_SELECT">.
  document.addEventListener("input", function (ev) {
    var campo = ev.target;
    if (!campo.dataset || !campo.dataset.filtrar) return;
    var select = document.getElementById(campo.dataset.filtrar);
    if (!select) return;
    if (!select._opcoes) select._opcoes = Array.prototype.slice.call(select.options);
    var termo = campo.value.trim().toLowerCase();
    var escolhido = select.value;
    select.innerHTML = "";
    select._opcoes.forEach(function (opcao, i) {
      if (i === 0 || opcao.value === escolhido || opcao.textContent.toLowerCase().indexOf(termo) !== -1) {
        select.appendChild(opcao);
      }
    });
    select.value = escolhido;
  });
  // Botão de tema (claro/escuro). A escolha fica neste navegador, na chave
  // "tema"; o <head> já aplica o tema salvo antes de desenhar a página.
  function iniciarTema() {
    var botao = document.getElementById("tema");
    if (!botao) return;
    var raiz = document.documentElement;
    var rotulo = botao.querySelector(".so-leitor");
    function atualizar() {
      var texto = raiz.dataset.theme === "dark" ? "Tema claro" : "Tema escuro";
      botao.title = texto;
      if (rotulo) rotulo.textContent = texto;
    }
    botao.addEventListener("click", function () {
      var novo = raiz.dataset.theme === "dark" ? "light" : "dark";
      raiz.dataset.theme = novo;
      try { localStorage.setItem("tema", novo); } catch (e) { /* sem armazenamento local */ }
      atualizar();
    });
    atualizar();
  }

  // Barra lateral recolhível (mesmo comportamento dos sistemas do hub).
  // Recolhida, vira uma faixa só com os ícones e a página ganha a largura. Com
  // o mouse (ou o foco do teclado) na faixa, a barra inteira abre por cima da
  // página, sem empurrar nada, e volta à faixa quando o mouse sai, com Esc ou
  // com um toque fora. A escolha fica salva neste navegador ("barra-recolhida").
  // Em tela estreita (menu empilhado em cima) a barra nunca fica compacta.
  var ESPERA_ABRIR = 120, ESPERA_FECHAR = 250; // ms: não pisca quando o mouse só passa
  function iniciarBarra() {
    var lateral = document.getElementById("lateral");
    var botao = document.getElementById("recolher");
    if (!lateral || !botao) return;
    var telaLarga = window.matchMedia("(min-width: 801px)");
    var recolhida = false, espiando = false, espera;
    try { recolhida = localStorage.getItem("barra-recolhida") === "1"; } catch (e) { /* sem armazenamento local */ }

    function aplicar() {
      var compacta = recolhida && !espiando && telaLarga.matches;
      lateral.classList.toggle("recolhida", recolhida);
      lateral.classList.toggle("espiando", recolhida && espiando);
      lateral.classList.toggle("compacta", compacta);
      var rotulo = recolhida ? "Fixar menu aberto" : "Recolher menu";
      botao.setAttribute("aria-label", rotulo);
      botao.title = rotulo;
      // na faixa, o nome de cada item aparece na dica
      lateral.querySelectorAll("#nav a, a#central").forEach(function (a) {
        if (compacta) a.title = a.textContent.trim();
        else a.removeAttribute("title");
      });
    }
    // no tablet não há "tirar o mouse": aberta pelo toque, fecha com um toque fora
    function aoTocarFora(ev) {
      if (!lateral.contains(ev.target)) { clearTimeout(espera); mudarEspiando(false); }
    }
    function mudarEspiando(abrir) {
      espiando = abrir;
      if (abrir) document.addEventListener("pointerdown", aoTocarFora);
      else document.removeEventListener("pointerdown", aoTocarFora);
      aplicar();
    }
    function espiar(abrir, atraso) {
      clearTimeout(espera);
      if (!recolhida || !telaLarga.matches) return;
      espera = setTimeout(function () { mudarEspiando(abrir); }, atraso);
    }
    lateral.addEventListener("mouseenter", function () { espiar(true, ESPERA_ABRIR); });
    lateral.addEventListener("mouseleave", function () { espiar(false, ESPERA_FECHAR); });
    // só o foco do teclado abre: o do clique, ou o que o navegador devolve ao
    // voltar para a aba, não pode reabrir a barra sozinho
    lateral.addEventListener("focusin", function (ev) { if (ev.target.matches(":focus-visible")) espiar(true, 0); });
    lateral.addEventListener("focusout", function (ev) { if (!lateral.contains(ev.relatedTarget)) espiar(false, 0); });
    lateral.addEventListener("keydown", function (ev) { if (ev.key === "Escape") espiar(false, 0); });
    botao.addEventListener("click", function () {
      clearTimeout(espera);
      recolhida = !recolhida;
      try { localStorage.setItem("barra-recolhida", recolhida ? "1" : "0"); } catch (e) { /* vale só nesta visita */ }
      mudarEspiando(false);
    });
    // a tela mudou de largura com a barra aberta por cima: desfaz
    telaLarga.addEventListener("change", function () { clearTimeout(espera); mudarEspiando(false); });
    aplicar();
  }

  iniciarTema();
  iniciarBarra();
})();
