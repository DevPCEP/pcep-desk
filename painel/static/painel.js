// PCEP Desk: botão "Copiar" do código de conexão e filtro simples de select.
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
})();
