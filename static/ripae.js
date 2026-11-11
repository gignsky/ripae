// Ripae 0.2 "Stowage" -- the terminal's behaviour, lifted from
// design/ripae-crt-mock.html and wired to htmx. Everything that appears or
// disappears paints line by line (raster), and within each painted block every
// row and field flickers on in turn (scanIn); overlays start with the screen
// flickering to black; prefers-reduced-motion makes all of it instant.
(function () {
  "use strict";
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.documentElement.classList.add("js");

  /* ---- scanlines within a block: every row, heading, field and tile in a lit block flickers on in
          turn, top to bottom, the way MU-TH-UR draws a reply. Outermost match wins (a tr, not its cells). ---- */
  var SCAN = "header, tr, li, h1, h2, h3, p, label, dl > div, .tile, .crate, .crate-tile, pre, .sub-h, .formfoot, .sale-fig, .sale-line, .sale-shares > div";
  var SCAN_STEP = 55, SCAN_BLOCK_MAX = 1800, SCAN_FLICKER = 320; // ms: per line, cap per block, one line's flicker
  function scanLines(block) {
    var all = Array.prototype.slice.call(block.querySelectorAll(SCAN));
    return all.filter(function (el) {
      if (el.closest("[hidden], template")) return false;
      for (var p = el.parentElement; p && p !== block; p = p.parentElement) if (all.indexOf(p) >= 0) return false;
      return true;
    });
  }
  function scanIn(block) {
    var subs = scanLines(block);
    if (!subs.length) { block.classList.add("scan"); block.style.animationDelay = "0ms"; subs = [block]; }
    var step = Math.min(SCAN_STEP, SCAN_BLOCK_MAX / subs.length), d = 0;
    subs.forEach(function (s, i) {
      s.classList.remove("scan"); void s.offsetWidth; // restart if it already ran (panel [SHOW] again)
      s.style.animationDelay = Math.round(d) + "ms"; s.classList.add("scan");
      d += (i % 3 === 2) ? step + 40 : step;
    });
    setTimeout(function () { subs.forEach(function (s) { s.classList.remove("scan"); s.style.animationDelay = ""; }); }, d + SCAN_FLICKER + 50);
    return d;
  }

  /* ---- slow raster: paint (or unpaint) a list of lines one tick at a time, uneven tick, no easing ---- */
  function raster(lines, on, done) {
    lines = lines.filter(function (l) { return l && l.isConnected !== false; });
    if (reduce) {
      lines.forEach(function (l) { l.classList.toggle("lit", on); l.removeAttribute("data-queued"); });
      if (done) done();
      return [];
    }
    var t = 0, timers = [];
    var seq = on ? lines : lines.slice().reverse();
    seq.forEach(function (l, i) {
      t += (i % 3 === 2) ? 210 : 120;
      var at = t;
      // painting in, a block's own lines take their time before the next block starts
      if (on) t += Math.min(scanLines(l).length * SCAN_STEP, SCAN_BLOCK_MAX);
      timers.push(setTimeout(function () {
        l.classList.add("flash"); l.classList.toggle("lit", on);
        if (on) scanIn(l);
        if (!on) { l.classList.add("lit"); } /* keep visible while flashing, then drop */
        timers.push(setTimeout(function () {
          l.classList.remove("flash"); if (!on) l.classList.remove("lit");
          l.removeAttribute("data-queued");
        }, 90));
      }, at));
    });
    if (done) timers.push(setTimeout(done, t + 120 + (on ? SCAN_FLICKER : 0)));
    return timers;
  }
  window.raster = raster;

  /* ---- paint everything new: htmx:load fires for the first page and every swapped-in node (oob included);
          htmx:afterSwap for the primary target. Lines already lit or queued are skipped, so both can fire. ---- */
  var SEL = ".ras, .rs";
  function unlitWithin(root) {
    if (!root || root.nodeType !== 1) return [];
    var out = [];
    if (root.matches(SEL)) out.push(root);
    root.querySelectorAll(SEL).forEach(function (l) { out.push(l); });
    return out.filter(function (l) {
      if (l.classList.contains("lit") || l.hasAttribute("data-queued")) return false;
      if (l.closest("#splash, #overlay, template")) return false; // overlays run their own sequence
      // a folded panel's lines are lit at once; unfolding has its own quick flash
      if (l.closest(".panel.fold:not(.open) > .body")) { l.classList.add("lit"); return false; }
      // nested lines (e.g. .ras inside a .ras tbody) light with their parent
      var p = l.parentElement && l.parentElement.closest(SEL);
      if (p && out.indexOf(p) >= 0) { l.classList.add("lit"); return false; }
      return true;
    });
  }
  function paintNew(root) {
    var lines = unlitWithin(root);
    lines.forEach(function (l) { l.setAttribute("data-queued", "1"); });
    if (lines.length) raster(lines, true);
  }
  document.addEventListener("htmx:load", function (e) { paintNew(e.detail && e.detail.elt); });
  document.addEventListener("htmx:afterSwap", function (e) { paintNew(e.detail && e.detail.target); });
  document.addEventListener("htmx:oobAfterSwap", function (e) { paintNew(e.detail && e.detail.target); });
  // a plain 4xx/5xx text body is shown, not swallowed
  document.addEventListener("htmx:beforeSwap", function (e) {
    var x = e.detail.xhr;
    if (x && x.status >= 400 && x.status !== 422 && x.responseText && x.responseText.indexOf("<") !== 0) {
      e.detail.shouldSwap = false;
      var f = document.querySelector(".foot > span:first-child");
      if (f) { f.innerHTML = '<span class="err">' + x.responseText + "</span>"; }
    }
  });

  /* ---- label every cell with its column header, so narrow screens can stack rows as cards ---- */
  function labelCards(root) {
    (root.matches && root.matches("table.cards") ? [root] : Array.prototype.slice.call(root.querySelectorAll ? root.querySelectorAll("table.cards") : [])).forEach(function (t) {
      // a table-wide thead, or (Open Cargo) the column-header row each voyage carries
      var hr = t.tHead ? t.tHead.rows[0] : t.querySelector("tr.colhead");
      if (!hr) return;
      var hs = Array.prototype.map.call(hr.cells, function (c) { return c.textContent.trim(); });
      Array.prototype.forEach.call(t.tBodies, function (tb) {
        Array.prototype.forEach.call(tb.rows, function (r) {
          if (r.cells.length < hs.length) return;
          Array.prototype.forEach.call(r.cells, function (c, i) { if (hs[i]) c.setAttribute("data-l", hs[i]); });
        });
      });
    });
    // a swapped-in tbody needs its table's headers
    if (root.matches && root.matches("tbody")) {
      var table = root.closest("table");
      if (table) labelCards(table);
    }
  }
  document.addEventListener("htmx:load", function (e) { if (e.detail && e.detail.elt) labelCards(e.detail.elt); });

  /* ---- blackout flicker ---- */
  var blackout = document.getElementById("blackout");
  function flick(fn) {
    if (!blackout || reduce) { fn(); return; }
    blackout.classList.remove("off"); blackout.classList.add("go");
    setTimeout(function () { fn(); blackout.classList.remove("go"); blackout.classList.add("off"); }, 900);
  }

  /* ---- overlays: SALE, DELETE, verdict arrive as fragments in #overlay ---- */
  var overlay = document.getElementById("overlay");
  if (overlay) {
    document.addEventListener("htmx:afterSwap", function (e) {
      if (e.detail.target !== overlay) return;
      // a form in an overlay answers a success with nothing: close it
      if (!overlay.innerHTML.trim()) { if (!overlay.hidden) flick(function () { overlay.hidden = true; }); return; }
      var parts = Array.prototype.slice.call(overlay.querySelectorAll(".ras"));
      parts.forEach(function (p) { p.classList.remove("lit"); });
      // already up (a form re-rendered with its error): repaint without the blackout
      if (!overlay.hidden) { raster(parts, true); return; }
      flick(function () { overlay.hidden = false; raster(parts, true); });
    });
    overlay.addEventListener("click", function (e) {
      var b = e.target.closest("[data-dismiss]");
      if (!b) return;
      flick(function () { overlay.hidden = true; overlay.innerHTML = ""; });
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && !overlay.hidden) flick(function () { overlay.hidden = true; overlay.innerHTML = ""; });
    });
  }

  /* ---- screens: raster out, navigate, raster in on the next page ---- */
  document.addEventListener("click", function (e) {
    var a = e.target.closest("a[data-go]");
    if (!a || a.classList.contains("on") || e.metaKey || e.ctrlKey || e.button) return;
    e.preventDefault();
    var href = a.getAttribute("href");
    var secs = Array.prototype.slice.call(document.querySelectorAll("main .rs.lit"));
    raster(secs, false, function () { location.href = href; });
  });

  /* ---- every panel header gets [HIDE]/[SHOW]; the body rasters out and in ---- */
  document.querySelectorAll(".panel > header").forEach(function (h) {
    var panel = h.parentNode;
    if (panel.classList.contains("fold") || panel.classList.contains("record")) return;
    var b = document.createElement("button"); b.type = "button"; b.className = "cmd minbtn"; b.textContent = "Hide"; b.setAttribute("aria-expanded", "true"); h.appendChild(b);
    b.addEventListener("click", function () {
      var body = Array.prototype.slice.call(panel.children).filter(function (c) { return c !== h; });
      if (panel.classList.contains("min")) {
        panel.classList.remove("min"); b.textContent = "Hide"; b.setAttribute("aria-expanded", "true");
        body.forEach(function (c) { c.classList.add("ras"); c.classList.remove("lit"); });
        raster(body, true);
      } else {
        body.forEach(function (c) { c.classList.add("ras", "lit"); });
        raster(body, false, function () { panel.classList.add("min"); b.textContent = "Show"; b.setAttribute("aria-expanded", "false"); });
      }
    });
  });

  /* ---- crate mix + price mode: live derivation on any load line (new run, add load, edit) ---- */
  function fmt(n) { return n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 }); }
  function fmt0(n) { return Math.round(n).toLocaleString("en-US"); }
  // "2×32, 4×8" or "2x32 4x8" -> [{n:2,size:32},{n:4,size:8}]
  function parseBoxes(str) {
    var out = [];
    String(str || "").replace(/\s*[x×X*]\s*/g, "x").split(/[,;\s]+/).forEach(function (tok) {
      var m = tok.match(/^(\d+)x(\d+)$/); if (m) out.push({ n: +m[1], size: +m[2] });
    });
    return out;
  }
  function wireLine(fields) {
    if (fields.dataset.wired) return; fields.dataset.wired = "1";
    var mode = fields.querySelector("[data-mode]"), scuL = fields.querySelector(".price-scu"), lotL = fields.querySelector(".price-lot");
    var scu = fields.querySelector("[data-scu]"), p = fields.querySelector("[name=price_scu]"), l = fields.querySelector("[name=price_lot]");
    var balL = Array.prototype.slice.call(fields.querySelectorAll(".price-bal"));
    var bb = fields.querySelector("[name=balance_before]"), ba = fields.querySelector("[name=balance_after]");
    var boxes = fields.querySelector("[data-boxes]"), bcalc = fields.querySelector(".boxes-calc");
    if (!mode || !p || !l) return; // the close form has its own wiring
    // SCU is generated from the boxes (crates define it): a read-only figure
    // the server derives again, so it is never typed or posted.
    var scuVal = parseFloat(scu && scu.textContent) || 0;
    function recalcBoxes() {
      if (!boxes) return;
      var bs = parseBoxes(boxes.value); var tot = 0, cnt = 0; bs.forEach(function (b) { tot += b.n * b.size; cnt += b.n; });
      scuVal = tot;
      if (scu) scu.textContent = bs.length ? String(tot) : "—";
      bcalc.innerHTML = bs.length ? "= <b>" + tot + "</b> SCU · " + cnt + " box" + (cnt === 1 ? "" : "es") : "= —";
    }
    function recalcPrice() {
      var q = scuVal;
      if (scuL) scuL.hidden = mode.value !== "scu"; if (lotL) lotL.hidden = mode.value !== "lot";
      balL.forEach(function (b) { b.hidden = mode.value !== "balance"; });
      if (mode.value === "scu" && scuL) { var per = parseFloat(p.value) || 0; scuL.querySelector(".calc").innerHTML = q && per ? "lot = <b>" + fmt(per * q) + "</b> · " + q + " SCU" : "lot = —"; }
      else if (mode.value === "balance" && bb && ba) {
        var bc = balL[balL.length - 1].querySelector(".calc");
        if (bb.value === "" || ba.value === "") { bc.textContent = "cost = —"; bc.classList.remove("bad"); return; }
        var cost = (parseFloat(bb.value) || 0) - (parseFloat(ba.value) || 0);
        bc.innerHTML = cost < 0 ? "balance went up -- swapped?" : "cost = <b>" + fmt0(cost) + "</b>" + (q ? " · per SCU = <b>" + fmt(cost / q) + "</b>" : "");
        bc.classList.toggle("bad", cost < 0);
      }
      else if (lotL) { var tot = parseFloat(l.value) || 0; lotL.querySelector(".calc").innerHTML = q && tot ? "per SCU = <b>" + fmt(tot / q) + "</b> · " + q + " SCU" : "per SCU = —"; }
    }
    if (boxes) boxes.addEventListener("input", function () { recalcBoxes(); recalcPrice(); });
    [mode, p, l, bb, ba].forEach(function (el) { if (el) { el.addEventListener("input", recalcPrice); el.addEventListener("change", recalcPrice); } });
    var status = fields.querySelector("[data-status]"), storedAt = fields.querySelector("[name=stored_at]");
    if (status && storedAt) {
      var sync = function () { var lab = storedAt.closest("label"); if (lab) lab.hidden = status.value !== "stored"; };
      status.addEventListener("change", sync); sync();
    }
    recalcBoxes(); recalcPrice();
  }
  function wireLines(root) {
    if (!root || !root.querySelectorAll) return;
    root.querySelectorAll(".fields").forEach(function (f) { if (f.querySelector("[data-mode]")) wireLine(f); });
  }
  document.addEventListener("htmx:load", function (e) { wireLines(e.detail && e.detail.elt); });

  /* ---- folding panels (the ledger screen): every panel starts as its title bar. Hover (a mouse), focus or a
          tap on the bar flashes it open. data-fold="stay" panels stay open until [HIDE]; "peek" (New Run) folds
          again when the pointer leaves unless it was clicked, focused into or typed into -- and never while one of
          its fields has focus. State is a class on the <section>, which no htmx swap replaces. Open "stay"
          panels are remembered for the tab (sessionStorage) and forgotten on entering through the splash. ---- */
  var FOLD_KEY = "ripae.panels.open";
  var FOLD_OPEN_MS = 200, FOLD_CLOSE_MS = 600; // hover before opening; how long New Run lingers after the pointer leaves
  var folds = Array.prototype.slice.call(document.querySelectorAll(".panel.fold"));
  function foldLoad() { try { return JSON.parse(sessionStorage.getItem(FOLD_KEY) || "[]"); } catch (e) { return []; } }
  function foldSave() {
    var ids = folds.filter(function (p) { return p.dataset.fold === "stay" && p.classList.contains("open"); }).map(function (p) { return p.id; });
    try { sessionStorage.setItem(FOLD_KEY, JSON.stringify(ids)); } catch (e) {}
  }
  function setFold(p, on, animate) {
    if (!on) p.classList.remove("pinned");
    if (p.classList.contains("open") === on) return;
    p.classList.toggle("open", on);
    var bar = p.querySelector(".bar"); if (bar) bar.setAttribute("aria-expanded", on ? "true" : "false");
    var body = p.querySelector(".body");
    if (on && body && animate !== false && !reduce) { body.classList.remove("unfold"); void body.offsetWidth; body.classList.add("unfold"); }
    if (p.dataset.fold === "stay") foldSave();
  }
  function foldAll() { folds.forEach(function (p) { setFold(p, false); }); }
  function fieldFocused(p) { var a = document.activeElement; return !!(a && p.contains(a) && a.matches("input, select, textarea")); }
  function peekClose(p) {
    if (p.dataset.fold !== "peek" || p.classList.contains("pinned") || fieldFocused(p)) return;
    setFold(p, false);
  }
  var hushFocus = false; // moving focus to the bar after [HIDE] must not unfold it again
  var splashMode0 = (document.getElementById("splash") || { dataset: {} }).dataset.mode;
  var keep = splashMode0 === "hidden" ? foldLoad() : []; // a landing (gate or welcome) starts folded
  folds.forEach(function (p) {
    var head = p.querySelector(":scope > header"), bar = p.querySelector(".bar"), hide = p.querySelector(".fold-hide");
    var peek = p.dataset.fold === "peek";
    p.classList.remove("open");
    if (bar) bar.setAttribute("aria-expanded", "false");
    if (keep.indexOf(p.id) >= 0 || p.classList.contains("pinned")) setFold(p, true, false);
    // hover intent: a pointer passing over a bar doesn't open it, and New Run lingers a moment after it
    // leaves, so a slip off the edge doesn't fold it. Focus, clicks and taps stay instant.
    var openT = 0, closeT = 0;
    function later(t, fn, ms) { clearTimeout(t); return setTimeout(fn, ms); }
    head.addEventListener("pointerenter", function (e) {
      if (e.pointerType === "touch") return;
      clearTimeout(closeT);
      openT = later(openT, function () { setFold(p, true); }, FOLD_OPEN_MS);
    });
    head.addEventListener("pointerleave", function () { clearTimeout(openT); });
    p.addEventListener("pointerenter", function () { clearTimeout(closeT); });
    p.addEventListener("pointerleave", function (e) {
      if (e.pointerType === "touch") return;
      clearTimeout(openT);
      closeT = later(closeT, function () { peekClose(p); }, FOLD_CLOSE_MS);
    });
    p.addEventListener("focusin", function (e) {
      if (hushFocus) return;
      setFold(p, true);
      if (peek && e.target !== bar && e.target !== hide) p.classList.add("pinned");
    });
    p.addEventListener("focusout", function (e) {
      if (peek && !(e.relatedTarget && p.contains(e.relatedTarget)) && !p.matches(":hover")) peekClose(p);
    });
    // a click (or tap) anywhere in it keeps it; typing in New Run does too
    p.addEventListener("click", function (e) {
      if (e.target.closest(".fold-hide, #newrun-cancel")) return;
      setFold(p, true); p.classList.add("pinned");
      if (peek && bar && e.target === bar) { var f = p.querySelector(".body input, .body select"); if (f) f.focus({ preventScroll: true }); }
    });
    p.addEventListener("input", function () { p.classList.add("pinned"); });
    function hideIt() { setFold(p, false); if (bar && p.contains(document.activeElement)) { hushFocus = true; bar.focus({ preventScroll: true }); hushFocus = false; } }
    if (hide) hide.addEventListener("click", hideIt);
    var cancel = peek && p.querySelector("#newrun-cancel");
    if (cancel) cancel.addEventListener("click", hideIt);
  });
  window.ripaeFoldAll = foldAll;

  /* ---- keyboard flow: focus the first field of a form that opens, and put focus somewhere
          sensible when an htmx swap takes away the element that had it ---- */
  var FIELDS = "input:not([type=hidden]):not([readonly]):not([disabled]):not([tabindex='-1']), select:not([disabled]), textarea:not([disabled])";
  function visible(el) { return !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length); }
  function firstField(root) { return Array.prototype.filter.call(root.querySelectorAll(FIELDS), visible)[0] || null; }
  // a swapped-in line is visibility:hidden until the raster lights it, and hidden things refuse focus
  function focusSoon(el, tries) {
    if (!el || !el.isConnected) return;
    var a = document.activeElement;
    if (tries && a && a !== document.body && a.isConnected) return; // the user has moved on
    el.focus({ preventScroll: true });
    if (document.activeElement !== el && (tries || 0) < 20) setTimeout(function () { focusSoon(el, (tries || 0) + 1); }, 60);
  }
  function focusFirstField(root) { var f = firstField(root); if (f) focusSoon(f); return !!f; }
  function buttonsIn(root) { return Array.prototype.filter.call(root.querySelectorAll("button:not([disabled])"), visible); }
  var lastTrigger = null, openers = {};
  document.addEventListener("htmx:beforeRequest", function (e) {
    var elt = e.detail && e.detail.elt, t = e.detail && e.detail.target;
    if (!elt || !t || !t.id || t.id === "overlay") { lastTrigger = null; return; }
    if (elt.tagName === "BUTTON" && !elt.closest("form")) {
      var scope = document.getElementById(t.id);
      lastTrigger = { scope: t.id, label: elt.textContent.trim(), index: scope ? buttonsIn(scope).indexOf(elt) : -1 };
    } else lastTrigger = { scope: t.id, fromForm: true };
  });
  document.addEventListener("htmx:afterSettle", function (e) {
    var trig = lastTrigger; lastTrigger = null;
    if (!trig) return;
    var a = document.activeElement;
    var lost = !a || a === document.body || !a.isConnected;
    var t = e.detail && e.detail.target, el = t && t.id ? document.getElementById(t.id) : null;
    if (!el) return;
    var form = el.querySelector("form");
    if (form) {
      // a form opened (or came back with an error): remember who opened it, start on its first field
      if (!trig.fromForm) openers[trig.scope] = trig;
      if (lost || !trig.fromForm || form.querySelector(".form-error")) focusFirstField(form);
      return;
    }
    if (!lost) return;
    // the form closed (saved or cancelled) or the trigger was replaced: back to the button that
    // opened it, else the same button again, else the row's first command
    var back = trig.fromForm ? openers[trig.scope] : trig;
    delete openers[trig.scope];
    var scope = (back && document.getElementById(back.scope)) || el;
    var bs = buttonsIn(scope);
    var hit = back && (bs.filter(function (b) { return b.textContent.trim() === back.label; })[0] || (back.index >= 0 && bs[back.index]));
    var first = hit || scope.querySelector(".cmd:not([disabled])") || bs[0];
    if (first) focusSoon(first);
  });
  /* ---- route editor (New Run, Edit voyage): stops, and every stop pick-list in step ---- */
  var stopSeq = 0;
  function shortTerminal(t) { return t.replace("Mining Facility ", "").replace("Mining Area ", ""); }
  function routeRows(ed) { return Array.prototype.slice.call(ed.querySelectorAll(".route-stops > .route-stop")); }
  // every row as {key, label}, start first -- labelled as views::stop_option_label spells it
  function routeOptions(ed) {
    return routeRows(ed).map(function (li, i) {
      var t = (li.querySelector("[name=stop_terminal]").value || "").trim();
      var name = t ? shortTerminal(t) : "—";
      return { key: li.querySelector("[name=stop_key]").value, label: (i === 0 ? "Start · " : "Stop " + i + " · ") + name };
    });
  }
  function fillPick(sel, opts) {
    var kind = sel.getAttribute("data-stop-pick");
    var want = sel.value || sel.getAttribute("data-sel") || "";
    var list = kind === "drop" ? opts.slice(1) : opts;
    var keys = list.map(function (o) { return o.key; });
    sel.innerHTML = "";
    if (kind === "drop") sel.appendChild(new Option("undecided", ""));
    list.forEach(function (o) { sel.appendChild(new Option(o.label, o.key)); });
    if (kind === "drop" && want === "" && sel.hasAttribute("data-fresh")) want = keys.length ? keys[keys.length - 1] : "";
    if (want !== "" && keys.indexOf(want) < 0) want = kind === "drop" ? (keys.length ? keys[keys.length - 1] : "") : (keys[0] || "");
    if (want === "" && kind !== "drop") want = keys[0] || "";
    sel.value = want; sel.removeAttribute("data-fresh"); sel.setAttribute("data-sel", want);
  }
  function refreshRoute(ed) {
    var rows = routeRows(ed);
    rows.forEach(function (li, i) {
      li.querySelector(".ln").textContent = i === 0 ? "Start" : i;
      var inp = li.querySelector("[name=stop_terminal]");
      inp.placeholder = i === 0 ? "where you start" : "terminal";
      inp.setAttribute("aria-label", i === 0 ? "Start" : "Stop " + i);
      var up = li.querySelector(".up"), rm = li.querySelector(".rm-stop");
      if (up) up.hidden = i === 0; if (rm) rm.hidden = i === 0;
    });
    var form = ed.closest("form") || ed.parentNode, opts = routeOptions(ed);
    form.querySelectorAll("select[data-stop-pick]").forEach(function (sel) { fillPick(sel, opts); });
  }
  function wireRoute(ed) {
    if (ed.dataset.wired) { refreshRoute(ed); return; } ed.dataset.wired = "1";
    var list = ed.querySelector(".route-stops"), tpl = ed.querySelector(".stop-tpl");
    ed.addEventListener("click", function (e) {
      var b = e.target.closest("button"); if (!b || !ed.contains(b)) return;
      var li = b.closest(".route-stop");
      if (b.hasAttribute("data-add-stop")) {
        var row = tpl.content.firstElementChild.cloneNode(true);
        row.querySelector("[name=stop_key]").value = "n" + Date.now().toString(36) + (stopSeq++);
        row.classList.add("lit"); list.appendChild(row);
        document.dispatchEvent(new CustomEvent("htmx:load", { detail: { elt: row } }));
        refreshRoute(ed); row.querySelector("[name=stop_terminal]").focus();
      } else if (b.classList.contains("rm-stop") && li && li.previousElementSibling) {
        li.remove(); refreshRoute(ed);
      } else if (b.classList.contains("up") && li && li.previousElementSibling) {
        list.insertBefore(li, li.previousElementSibling); refreshRoute(ed);
      }
    });
    // keep the labels in step with the terminals as they're typed or picked
    ed.addEventListener("input", function (e) { if (e.target.name === "stop_terminal") refreshRoute(ed); });
    ed.addEventListener("change", function (e) { if (e.target.name === "stop_terminal") refreshRoute(ed); });
    refreshRoute(ed);
  }
  function wireRoutes(root) {
    if (!root || !root.querySelectorAll) return;
    var eds = Array.prototype.slice.call(root.querySelectorAll("[data-route]"));
    // a load line added to a form that has a route: fill its pick-lists
    var form = root.closest && root.closest("form");
    var own = form && form.querySelector("[data-route]");
    if (own && eds.indexOf(own) < 0) eds.push(own);
    eds.forEach(wireRoute);
  }
  document.addEventListener("htmx:load", function (e) { wireRoutes(e.detail && e.detail.elt); });

  /* ---- New Run panel: add / remove load lines ---- */
  var panel = document.getElementById("newrun");
  if (panel) {
    var loads = document.getElementById("loads"), tpl = document.getElementById("load-tpl");
    function renumber() {
      Array.prototype.forEach.call(loads.children, function (c, i) {
        c.querySelector(".ln").textContent = i + 1;
        var rm = c.querySelector(".rm"); if (rm) rm.parentNode.hidden = loads.children.length === 1;
      });
    }
    function wireRemove(line) {
      var rm = line.querySelector(".rm");
      if (rm) rm.addEventListener("click", function () {
        if (loads.children.length === 1) return;
        raster([line], false, function () { line.remove(); renumber(); });
      });
    }
    function addLoad() {
      var line = tpl.content.firstElementChild.cloneNode(true);
      // a new line is picked up at the start and dropped at the last stop (static/ripae.js route editor)
      var dsel = line.querySelector("select[data-stop-pick=drop]"); if (dsel) dsel.setAttribute("data-fresh", "1");
      loads.appendChild(line); wireRemove(line); renumber();
      // let the picker, the date default and the price wiring see the new line
      document.dispatchEvent(new CustomEvent("htmx:load", { detail: { elt: line } }));
      raster([line], true);
      focusFirstField(line);
    }
    Array.prototype.forEach.call(loads.children, wireRemove); renumber();
    var add = document.getElementById("add-load"); if (add) add.addEventListener("click", addLoad);
  }

  /* ---- close form: crate picker, live preview, shares ---- */
  function wireClose(form) {
    if (form.dataset.wired) return; form.dataset.wired = "1";
    var buyScu = parseFloat(String(form.dataset.buyScu || "0").replace(/,/g, "")) || 0;
    var scuSold = form.querySelector("[data-scu-sold]"), mode = form.querySelector("[data-mode]");
    var scuL = form.querySelector(".price-scu"), lotL = form.querySelector(".price-lot");
    var balL = Array.prototype.slice.call(form.querySelectorAll(".price-bal"));
    var sharesBox = form.querySelector(".shares");
    function crates() { return Array.prototype.slice.call(form.querySelectorAll("label.crate")); }
    function paintCrate(c) {
      var i = c.querySelector("input"); var sk = c.querySelector(".sold-kept");
      c.classList.toggle("kept", !i.checked && !i.disabled);
      if (sk) sk.textContent = i.checked ? "sold" : "kept";
    }
    function syncScu() {
      if (form.dataset.scu) return +form.dataset.scu; // the sale editor: the crates already sold
      var q = 0; crates().forEach(function (c) { var i = c.querySelector("input"); if (i.checked && !i.disabled) q += +c.dataset.size; });
      if (scuSold) scuSold.value = q; return q;
    }
    function revenue() {
      var q = syncScu();
      if (mode.value === "scu") { var per = parseFloat(form.querySelector("[name=sell_scu]").value) || 0; return { q: q, rev: per * q, per: per }; }
      if (mode.value === "balance") {
        var b = form.querySelector("[name=sell_bal_before]").value, a = form.querySelector("[name=sell_bal_after]").value;
        var d = b === "" || a === "" ? 0 : (parseFloat(a) || 0) - (parseFloat(b) || 0);
        return { q: q, rev: d, per: q ? d / q : 0 };
      }
      var tot = parseFloat(form.querySelector("[name=sell_lot]").value) || 0; return { q: q, rev: tot, per: q ? tot / q : 0 };
    }
    function shares(profit) {
      var paid = 0;
      sharesBox.querySelectorAll(".share").forEach(function (sh) {
        var kind = sh.querySelector("[name=share_kind]").value, amt = parseFloat(sh.querySelector("[name=share_amount]").value) || 0;
        paid += profit <= 0 ? 0 : (kind === "pct" ? profit * amt / 100 : amt);
      });
      return paid;
    }
    function recalc() {
      if (scuL) scuL.hidden = mode.value !== "scu"; if (lotL) lotL.hidden = mode.value !== "lot";
      balL.forEach(function (b) { b.hidden = mode.value !== "balance"; });
      var r = revenue();
      if (mode.value === "scu") scuL.querySelector(".calc").innerHTML = r.rev ? "revenue = <b>" + fmt0(r.rev) + "</b> · " + r.q + " SCU" : "revenue = —";
      else if (mode.value === "balance") {
        var bc = balL[balL.length - 1].querySelector(".calc");
        bc.innerHTML = r.rev < 0 ? "balance went down -- swapped?" : r.rev ? "revenue = <b>" + fmt0(r.rev) + "</b>" + (r.q ? " · per SCU = <b>" + fmt(r.per) + "</b>" : "") : "revenue = —";
        bc.classList.toggle("bad", r.rev < 0);
      }
      else lotL.querySelector(".calc").innerHTML = r.rev ? "per SCU = <b>" + fmt(r.per) + "</b> · " + r.q + " SCU" : "per SCU = —";
      var profit = r.rev - buyScu * r.q; var paid = shares(profit);
      var pv = form.querySelector(".preview");
      pv.innerHTML = r.rev ? ('profit = <b class="' + (profit < 0 ? "neg" : "pos") + '">' + (profit < 0 ? "−" : "+") + fmt0(Math.abs(profit)) + "</b> · shares " + fmt0(paid) + " · seller keeps <b>" + fmt0(profit - paid) + "</b>") : "profit = — · seller keeps —";
    }
    crates().forEach(function (c) { paintCrate(c); c.querySelector("input").addEventListener("change", function () { paintCrate(c); recalc(); }); });
    var all = form.querySelector(".sel-all"), none = form.querySelector(".sel-none");
    if (all) all.addEventListener("click", function () { crates().forEach(function (c) { var i = c.querySelector("input"); if (!i.disabled) i.checked = c.dataset.sound === "1"; paintCrate(c); }); recalc(); });
    if (none) none.addEventListener("click", function () { crates().forEach(function (c) { var i = c.querySelector("input"); if (!i.disabled) i.checked = false; paintCrate(c); }); recalc(); });
    wireShares(form, recalc);
    form.querySelectorAll("input,select").forEach(function (el) { el.addEventListener("input", recalc); el.addEventListener("change", recalc); });
    recalc();
  }

  /* ---- profit shares: [+ ADD SHARE] / [REMOVE], even split. Close form, sale editor, voyage form ---- */
  function wireShares(form, onChange) {
    if (form.dataset.sharesWired) return; form.dataset.sharesWired = "1";
    var box = form.querySelector(".shares"), tpl = form.querySelector(".share-tpl"), add = form.querySelector(".add-share");
    if (!box || !tpl || !add) return;
    onChange = onChange || function () {};
    function rows() { return Array.prototype.slice.call(box.querySelectorAll(".share")); }
    // whoever keeps the remainder: the seller picked on a sale, the run's owner on a voyage
    function owner() { var s = form.querySelector("[name=sold_by], [name=logged_by]"); return s ? s.value : form.dataset.owner || ""; }
    // a % share nobody has typed into is an even cut: what the typed-in %
    // shares leave, split equally among them and whoever keeps the remainder
    function even() {
      var typed = 0, auto = [];
      rows().forEach(function (sh) {
        if (sh.querySelector("[name=share_kind]").value !== "pct") return;
        var a = sh.querySelector("[name=share_amount]");
        if (sh.dataset.auto) auto.push(a); else typed += parseFloat(a.value) || 0;
      });
      if (!auto.length) return;
      var each = Math.floor(Math.max(0, 100 - typed) / (auto.length + 1) * 100) / 100;
      auto.forEach(function (a) { a.value = each; });
    }
    // a new share goes to the first crew member without one
    function pickCrew(sh) {
      var taken = rows().filter(function (r) { return r !== sh; }).map(function (r) { return r.querySelector("[name=share_to]").value; });
      taken.push(owner());
      var sel = sh.querySelector("[name=share_to]");
      var free = Array.prototype.filter.call(sel.options, function (o) { return taken.indexOf(o.value) < 0; })[0];
      if (free) sel.value = free.value;
    }
    function wire(sh) {
      sh.querySelectorAll("input,select").forEach(function (el) { el.addEventListener("input", onChange); el.addEventListener("change", onChange); });
      sh.querySelector("[name=share_amount]").addEventListener("input", function () { delete sh.dataset.auto; even(); onChange(); });
      sh.querySelector("[name=share_kind]").addEventListener("change", function () { even(); onChange(); });
      sh.querySelector(".rm-share").addEventListener("click", function () { raster([sh], false, function () { sh.remove(); even(); onChange(); }); });
    }
    rows().forEach(wire);
    add.addEventListener("click", function () {
      var sh = tpl.content.firstElementChild.cloneNode(true); box.appendChild(sh);
      pickCrew(sh); wire(sh); even(); raster([sh], true); onChange();
    });
  }
  document.addEventListener("htmx:load", function (e) {
    var root = e.detail && e.detail.elt; if (!root || !root.querySelectorAll) return;
    if (root.matches && root.matches("form.close-form")) wireClose(root);
    root.querySelectorAll("form.close-form").forEach(wireClose);
    if (root.matches && root.matches("form.shares-form")) wireShares(root);
    root.querySelectorAll("form.shares-form").forEach(function (f) { wireShares(f); });
  });

  /* ---- live updates: another crew member's change refreshes the ledger panels in place ----
          src/live.rs announces every write on GET /live; this tab tags its own requests so it can
          skip its own announcements. Rows with an open form or crate manifest are kept as they are;
          rows whose text didn't change stay lit, so only what changed paints in. ---- */
  var TAB = Math.random().toString(36).slice(2) + Date.now().toString(36);
  document.addEventListener("htmx:configRequest", function (e) { e.detail.headers["X-Ripae-Tab"] = TAB; });
  if (document.getElementById("open-cargo") && window.EventSource) {
    var inflight = 0, pending = false, fetching = false, last = {};
    document.addEventListener("htmx:beforeRequest", function () { inflight++; });
    document.addEventListener("htmx:afterRequest", function () { inflight = Math.max(0, inflight - 1); if (!inflight && pending) refresh(); });
    var norm = function (el) { return el.textContent.replace(/\s+/g, " ").trim(); };
    var busy = function (el) { return !!el.querySelector("form, tr.manifest"); };
    function footNote(msg) {
      var f = document.querySelector(".foot > span:first-child");
      if (f) f.innerHTML = '<span class="err">' + msg + "</span>";
    }
    function merge(old, neu) {
      // keep anything being worked on; note what someone else closed or deleted under you
      Array.prototype.forEach.call(old.querySelectorAll("tbody[id]"), function (o) {
        if (!busy(o)) return;
        var n = neu.querySelector("#" + o.id);
        if (n) n.replaceWith(o);
        else footNote("A row you had open was changed by another crew member");
      });
      // unchanged lines don't repaint
      var seen = {};
      Array.prototype.forEach.call(old.querySelectorAll(".ras, .rs"), function (o) { var k = norm(o); seen[k] = (seen[k] || 0) + 1; });
      Array.prototype.forEach.call(neu.querySelectorAll(".ras, .rs"), function (n) {
        var k = norm(n); if (seen[k] && !n.classList.contains("lit")) { seen[k]--; n.classList.add("lit"); }
      });
      if (old.classList.contains("lit")) neu.classList.add("lit");
      old.replaceWith(neu);
      htmx.process(neu);
      document.dispatchEvent(new CustomEvent("htmx:load", { detail: { elt: neu } }));
    }
    function refresh() {
      var ov = document.getElementById("overlay");
      // don't pull rows out from under a request in flight or behind an open overlay
      if (inflight || fetching || (ov && !ov.hidden)) { pending = true; return; }
      pending = false; fetching = true;
      fetch("/live/panels", { headers: { "X-Ripae-Tab": TAB }, credentials: "same-origin" })
        .then(function (r) { if (!r.ok) throw new Error(r.status); return r.text(); })
        .then(function (text) {
          var tpl = document.createElement("template"); tpl.innerHTML = text;
          // compare with the markup the server sent last time: the live DOM carries lit classes,
          // card labels and the like, and an attribute-only change (a note) has no text to compare
          Array.prototype.slice.call(tpl.content.children).forEach(function (neu) {
            var old = neu.id && document.getElementById(neu.id), src = neu.outerHTML;
            if (old && last[neu.id] !== src) { last[neu.id] = src; merge(old, neu); }
          });
        })
        .catch(function () {})
        .then(function () { fetching = false; if (pending) setTimeout(refresh, 250); });
    }
    // a refresh held back by an overlay runs once it's dismissed (after the flicker)
    var retry = function () { setTimeout(function () { if (pending) refresh(); }, 1200); };
    if (overlay) overlay.addEventListener("click", function (e) { if (e.target.closest("[data-dismiss]")) retry(); });
    document.addEventListener("htmx:afterSwap", function (e) { if (e.detail.target === overlay && !overlay.innerHTML.trim()) retry(); });
    document.addEventListener("keydown", function (e) { if (e.key === "Escape") retry(); });
    var timer = null, dropped = false;
    var es = new EventSource("/live");
    es.addEventListener("changed", function (e) {
      if (e.data === TAB) return;
      clearTimeout(timer); timer = setTimeout(refresh, 200); // coalesce a burst of writes
    });
    // after a dropped connection, catch up on whatever was missed
    es.addEventListener("error", function () { dropped = true; });
    es.addEventListener("open", function () { if (dropped) { dropped = false; refresh(); } });
  }

  /* ---- splash: boot log, then the gate or the welcome line ---- */
  var splash = document.getElementById("splash");
  if (splash) {
    var boot = document.getElementById("boot");
    var mode0 = splash.dataset.mode; // gate | welcome | hidden (hidden = welcome on replay)
    var who = splash.dataset.who || "";
    var bootBase = JSON.parse(splash.dataset.boot || "[]");
    var splashTimers = [], armed = false;
    function parts() { return Array.prototype.slice.call(splash.querySelectorAll(".ras")).filter(function (p) { return !p.hidden && !p.closest("[hidden]"); }); }
    function runSplash(gate) {
      splashTimers.forEach(clearTimeout); splashTimers = []; armed = false;
      splash.hidden = false; boot.textContent = "";
      var g = document.getElementById("gate"), k = document.getElementById("g-known");
      if (g) g.hidden = !gate; if (k) k.hidden = gate;
      var lines = bootBase.concat([gate ? "IDENTIFY TO PROCEED" : "SESSION VALID · " + who]);
      var ps = parts(); ps.forEach(function (p) { p.classList.remove("lit"); });
      var instant = splash.dataset.error === "1"; // a wrong pair: don't replay the whole boot
      splash.dataset.error = "0";
      raster(ps.slice(0, 2), true, function () {
        boot.classList.add("lit");
        var t = 0;
        lines.forEach(function (l, i) { t += (reduce || instant) ? 0 : (i === lines.length - 1 ? 420 : 160); splashTimers.push(setTimeout(function () { boot.textContent += l + "\n"; }, t)); });
        splashTimers.push(setTimeout(function () {
          raster(ps.slice(3), true, function () { var h = gate ? document.getElementById("g-discord") || document.getElementById("g-handle") : cont; if (h) h.focus({ preventScroll: true }); armed = true; });
        }, t + 150));
      });
    }
    // entering (again) through the splash starts the ledger's panels folded
    function dismiss() {
      if (!armed) return; armed = false;
      foldAll();
      raster(parts(), false, function () { splash.hidden = true; });
    }
    var cont = document.getElementById("g-continue");
    if (cont) cont.addEventListener("click", function (e) {
      e.stopPropagation(); splashTimers.forEach(clearTimeout); splashTimers = []; armed = true; dismiss();
    });
    // with a session, any key or click lets you through; without one, only the gate does
    ["click", "touchstart"].forEach(function (ev) {
      splash.addEventListener(ev, function (e) { if (mode0 !== "gate" && !e.target.closest(".cmd, button, a, input")) dismiss(); });
    });
    document.addEventListener("keydown", function (e) { if (!splash.hidden && mode0 !== "gate" && e.key !== "Tab" && e.target.id !== "g-continue") dismiss(); });
    var replay = document.getElementById("replay-splash");
    // blur first: a Space/Enter on the still-focused button would otherwise re-run the splash on keyup
    if (replay) replay.addEventListener("click", function (e) { e.stopPropagation(); replay.blur(); runSplash(mode0 === "gate"); });
    if (mode0 === "gate" || mode0 === "welcome") runSplash(mode0 === "gate");
    else splash.hidden = true;
  }

  /* ---- leaderboard meters ---- */
  document.querySelectorAll(".meter[data-pct]").forEach(function (m) { m.style.setProperty("--p", m.dataset.pct); });
})();
