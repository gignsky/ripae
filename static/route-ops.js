// Route Ops (0.3): [Peek] and the spine's stop-detail popover are pure
// client state -- never persisted (RIPAE-0.3-ROUTE-OPS.md §2). Every real
// mutation (advance, retreat, stow, store, sell) redraws the whole
// #route-ops-cards fragment out of band, which resets both back to the
// phase-driven default; that's wanted, since the phase may just have
// changed. Runs standalone on this page only.
(function () {
  "use strict";

  function applyFace(card) {
    var auto = card.dataset.autoFace;
    var peeking = card.dataset.peek === "1";
    var shown = peeking ? (auto === "stow" ? "track" : "stow") : auto;
    var faces = card.querySelectorAll(".face");
    for (var i = 0; i < faces.length; i++) {
      faces[i].classList.toggle("show", faces[i].dataset.face === shown);
    }
  }

  function setUp(root) {
    var cards = root.querySelectorAll ? root.querySelectorAll(".voyage-card") : [];
    for (var i = 0; i < cards.length; i++) applyFace(cards[i]);
  }

  document.addEventListener("click", function (e) {
    var card = e.target.closest && e.target.closest(".voyage-card");
    if (!card) return;
    if (e.target.closest("[data-peek-on]")) {
      card.dataset.peek = "1";
      applyFace(card);
      return;
    }
    if (e.target.closest("[data-peek-off]")) {
      card.dataset.peek = "0";
      applyFace(card);
      return;
    }
    var node = e.target.closest("[data-node]");
    if (node) {
      var n = node.dataset.node;
      var already = card.dataset.openNode === n;
      card.dataset.openNode = already ? "" : n;
      var details = card.querySelectorAll(".stop-detail");
      for (var j = 0; j < details.length; j++) {
        details[j].classList.toggle("show", !already && details[j].dataset.n === n);
      }
    }
  });

  // new cards (initial load, or an out-of-band redraw after a mutation)
  // start from their phase's own face and no node open.
  document.addEventListener("htmx:load", function (e) {
    var root = e.detail && e.detail.elt;
    if (root) setUp(root);
  });
  setUp(document);
})();
