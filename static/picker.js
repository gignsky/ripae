// Fuzzy pick-list for inputs marked `data-pick list="<datalist id>"`.
// The datalist is the source of truth (and the no-JS fallback); this swaps
// the native suggestions for a fuzzy-ranked dropdown. Since 0.2 the lists
// are suggestions, not gates: a name on the list snaps to the list's
// spelling, any other name is kept as typed (the server does the same).
(function () {
  "use strict";

  var MAX_RESULTS = 50;

  // Higher is better; null means no match. Substring hits beat scattered
  // subsequence hits; word starts and consecutive letters score extra.
  function score(query, text) {
    var q = query.toLowerCase().replace(/\s+/g, " ").trim();
    var t = text.toLowerCase();
    if (!q) return 0;
    var at = t.indexOf(q);
    if (at >= 0) {
      var wordStart = at === 0 || /[\s\-(]/.test(t[at - 1]);
      return 1000 + (wordStart ? 100 : 0) - at - t.length * 0.1;
    }
    var total = 0, from = 0, prev = -2;
    for (var i = 0; i < q.length; i++) {
      var ch = q[i];
      if (ch === " ") continue;
      var found = t.indexOf(ch, from);
      if (found < 0) return null;
      total += 1;
      if (found === prev + 1) total += 5;
      if (found === 0 || /[\s\-(]/.test(t[found - 1])) total += 8;
      prev = found;
      from = found + 1;
    }
    return total - t.length * 0.1;
  }

  // Bold the matched letters (substring run, else each subsequence letter).
  function highlight(li, query, text) {
    var q = query.toLowerCase().replace(/\s+/g, " ").trim();
    var t = text.toLowerCase();
    var marks = {};
    var at = q ? t.indexOf(q) : -1;
    if (at >= 0) {
      for (var k = at; k < at + q.length; k++) marks[k] = true;
    } else {
      var from = 0;
      for (var i = 0; i < q.length; i++) {
        if (q[i] === " ") continue;
        var f = t.indexOf(q[i], from);
        if (f < 0) break;
        marks[f] = true;
        from = f + 1;
      }
    }
    li.textContent = "";
    for (var j = 0; j < text.length; j++) {
      if (marks[j]) {
        var m = document.createElement("mark");
        m.textContent = text[j];
        li.appendChild(m);
      } else {
        li.appendChild(document.createTextNode(text[j]));
      }
    }
  }

  function enhance(input) {
    if (input.dataset.pickReady) return;
    var listId = input.getAttribute("list");
    var source = listId && document.getElementById(listId);
    if (!source) return;
    input.dataset.pickReady = "1";

    var options = Array.prototype.map.call(source.options, function (o) {
      return o.value;
    });
    input.removeAttribute("list"); // suppress the native dropdown

    var wrap = document.createElement("div");
    wrap.className = "pick";
    input.parentNode.insertBefore(wrap, input);
    wrap.appendChild(input);

    var menu = document.createElement("ul");
    menu.className = "pick-menu";
    menu.setAttribute("role", "listbox");
    menu.hidden = true;
    wrap.appendChild(menu);

    var shown = [];
    var active = -1;

    function canonical(value) {
      var v = value.trim().toLowerCase();
      for (var i = 0; i < options.length; i++) {
        if (options[i].toLowerCase() === v) return options[i];
      }
      return null;
    }

    function validate() {
      // names are suggestions, not gates (ruled 8 Nov 2026): nothing to refuse
      input.setCustomValidity("");
      input.removeAttribute("aria-invalid");
    }

    function setActive(i) {
      active = i;
      Array.prototype.forEach.call(menu.children, function (li, j) {
        li.setAttribute("aria-selected", j === i ? "true" : "false");
        if (j === i) li.scrollIntoView({ block: "nearest" });
      });
    }

    // `query` defaults to what is typed; focus passes "" to show the whole list.
    function render(query) {
      var q = typeof query === "string" ? query : input.value;
      shown = options
        .map(function (name) { return { name: name, s: score(q, name) }; })
        .filter(function (r) { return r.s !== null; })
        .sort(function (a, b) { return b.s - a.s || a.name.localeCompare(b.name); })
        .slice(0, MAX_RESULTS)
        .map(function (r) { return r.name; });
      menu.textContent = "";
      shown.forEach(function (name) {
        var li = document.createElement("li");
        li.setAttribute("role", "option");
        highlight(li, q, name);
        li.addEventListener("mousedown", function (e) {
          e.preventDefault(); // keep focus in the input
          choose(name);
        });
        menu.appendChild(li);
      });
      menu.style.top = input.offsetTop + input.offsetHeight + "px";
      menu.hidden = shown.length === 0;
      setActive(q.trim() && shown.length ? 0 : -1);
    }

    function close() {
      menu.hidden = true;
      active = -1;
    }

    function choose(name) {
      input.value = name;
      close();
      validate();
    }

    input.addEventListener("focus", function () {
      // A pre-filled pick (e.g. the default terminal) is selected so typing
      // replaces it, and the menu opens on the full list rather than one hit.
      if (canonical(input.value)) {
        input.select();
        render("");
      } else {
        render();
      }
    });
    input.addEventListener("input", function () {
      render();
      validate();
    });
    input.addEventListener("blur", function () {
      close();
      var exact = canonical(input.value);
      if (exact) input.value = exact; // snap to the list's spelling
      validate();
    });
    input.addEventListener("keydown", function (e) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (menu.hidden) render();
        if (!shown.length) return;
        e.preventDefault();
        var step = e.key === "ArrowDown" ? 1 : -1;
        setActive((active + step + shown.length) % shown.length);
      } else if (e.key === "Enter" && !menu.hidden && active >= 0) {
        e.preventDefault(); // pick, don't submit (edit/close are htmx forms)
        choose(shown[active]);
      } else if (e.key === "Escape" && !menu.hidden) {
        e.preventDefault();
        close();
      } else if (e.key === "Tab" && !menu.hidden && active >= 0 && input.value.trim()) {
        choose(shown[active]);
      }
    });

    validate();
  }

  function enhanceWithin(root) {
    if (!root || !root.querySelectorAll) return;
    if (root.matches && root.matches("input[data-pick]")) enhance(root);
    root.querySelectorAll("input[data-pick]").forEach(enhance);
  }

  // htmx fires `htmx:load` for the initial page and every swapped-in
  // fragment (edit/close rows); DOMContentLoaded covers pages without it.
  document.addEventListener("htmx:load", function (e) {
    enhanceWithin(e.detail && e.detail.elt);
  });
  document.addEventListener("DOMContentLoaded", function () {
    enhanceWithin(document.body);
  });
})();
