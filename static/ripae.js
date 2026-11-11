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
      if (!t.tHead) return;
      var hs = Array.prototype.map.call(t.tHead.rows[0].cells, function (c) { return c.textContent.trim(); });
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

  /* ---- [data-expand]: show / hide the element it names (a member ledger's loads under a voyage) ---- */
  document.addEventListener("click", function (e) {
    var b = e.target.closest("[data-expand]"); if (!b) return;
    var t = document.getElementById(b.dataset.expand); if (!t) return;
    var open = t.hidden; t.hidden = !open; b.setAttribute("aria-expanded", open ? "true" : "false");
    b.classList.toggle("on", open);
    if (open) { t.classList.add("ras"); t.classList.remove("lit"); raster([t], true); }
  });

  /* ---- every panel header gets [HIDE]/[SHOW]; the body rasters out and in ---- */
  document.querySelectorAll(".panel > header").forEach(function (h) {
    var panel = h.parentNode;
    if (panel.id === "newrun" || panel.classList.contains("record")) return;
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
    var scuTouched = !!(scu && scu.value);
    function recalcBoxes() {
      if (!boxes) return;
      var bs = parseBoxes(boxes.value); var tot = 0, cnt = 0; bs.forEach(function (b) { tot += b.n * b.size; cnt += b.n; });
      if (!bs.length) { bcalc.textContent = "= —"; bcalc.classList.remove("bad"); return; }
      if (!scuTouched || !scu.value) scu.value = tot;
      var q = parseFloat(scu.value) || 0;
      bcalc.innerHTML = "= <b>" + tot + "</b> SCU · " + cnt + " box" + (cnt === 1 ? "" : "es") + (q && q !== tot ? " · SCU field says " + q + " -- crates define the SCU" : "");
      bcalc.classList.toggle("bad", !!q && q !== tot);
    }
    function recalcPrice() {
      var q = parseFloat(scu && scu.value) || 0;
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
    if (scu) scu.addEventListener("input", function () { scuTouched = true; recalcBoxes(); recalcPrice(); });
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

  /* ---- New Run panel: [NEW RUN]/[CLOSE], add / remove load lines ---- */
  var panel = document.getElementById("newrun");
  if (panel) {
    var toggle = document.getElementById("newrun-toggle"), cancel = document.getElementById("newrun-cancel");
    var body = document.getElementById("newrun-body"), loads = document.getElementById("loads"), tpl = document.getElementById("load-tpl");
    var timers = [];
    function clear() { timers.forEach(clearTimeout); timers = []; }
    function allLines() { return Array.prototype.slice.call(panel.querySelectorAll(".ras")); }
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
      // the new line inherits the previous line's terminal
      var prev = loads.lastElementChild;
      if (prev) { var t = prev.querySelector("[name=terminal]"); var nt = line.querySelector("[name=terminal]"); if (t && nt) nt.value = t.value; }
      loads.appendChild(line); wireRemove(line); renumber();
      // let the picker, the date default and the price wiring see the new line
      document.dispatchEvent(new CustomEvent("htmx:load", { detail: { elt: line } }));
      raster([line], true);
    }
    Array.prototype.forEach.call(loads.children, wireRemove); renumber();
    function open() {
      clear(); panel.classList.remove("collapsed"); body.hidden = false;
      toggle.setAttribute("aria-expanded", "true"); toggle.textContent = "Close";
      var lines = allLines(); lines.forEach(function (l) { l.classList.remove("lit", "flash"); });
      timers = raster(lines, true, function () { var f = panel.querySelector("input"); if (f) f.focus({ preventScroll: true }); });
    }
    function close() {
      clear(); toggle.setAttribute("aria-expanded", "false"); toggle.textContent = "New run";
      timers = raster(allLines(), false, function () { panel.classList.add("collapsed"); body.hidden = true; });
    }
    toggle.addEventListener("click", function () { panel.classList.contains("collapsed") ? open() : close(); });
    if (cancel) cancel.addEventListener("click", close);
    var add = document.getElementById("add-load"); if (add) add.addEventListener("click", addLoad);
    if (!panel.classList.contains("collapsed")) { allLines().forEach(function (l) { l.classList.add("lit"); }); }
  }

  /* ---- close form: crate picker, live preview, the crew's split ---- */
  function wireClose(form) {
    if (form.dataset.wired) return; form.dataset.wired = "1";
    var buyScu = parseFloat(String(form.dataset.buyScu || "0").replace(/,/g, "")) || 0;
    var scuSold = form.querySelector("[data-scu-sold]"), mode = form.querySelector("[data-mode]");
    var scuL = form.querySelector(".price-scu"), lotL = form.querySelector(".price-lot");
    var balL = Array.prototype.slice.call(form.querySelectorAll(".price-bal"));
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
    // the split shown read-only: the crew's terms less the seller's own (or, on the
    // sale editor, the shares as recorded unless re-split by the crew)
    var resplit = form.querySelector(".resplit");
    function shares(profit) {
      var paid = 0, seller = (form.querySelector("[name=sold_by]") || {}).value;
      var crew = form.querySelector(".crew-split"), rec = form.querySelector(".split-lines.recorded");
      if (rec && resplit) { rec.hidden = resplit.checked; crew.hidden = !resplit.checked; }
      form.querySelectorAll(".split-lines").forEach(function (box) {
        box.querySelectorAll(".split-line").forEach(function (ln) {
          var skip = box.dataset.skipSeller && ln.dataset.user === seller;
          ln.classList.toggle("dim", !!skip);
          var tag = ln.querySelector(".seller-tag"); if (tag) tag.hidden = !skip;
          if (box.hidden || skip || profit <= 0) return;
          paid += profit * (parseFloat(ln.dataset.pct) || 0) / 100 + (parseFloat(ln.dataset.flat) || 0);
        });
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
    form.querySelectorAll("input,select").forEach(function (el) { el.addEventListener("input", recalc); el.addEventListener("change", recalc); });
    recalc();
  }

  /* ---- voyage crew: [+ ADD CREW] / [REMOVE]; a new member's % is an even cut until typed into ---- */
  function wireCrew(form) {
    if (form.dataset.crewWired) return; form.dataset.crewWired = "1";
    var box = form.querySelector(".crew-rows"), tpl = form.querySelector(".crew-tpl"), add = form.querySelector(".add-crew");
    if (!box || !tpl || !add) return;
    function rows() { return Array.prototype.slice.call(box.querySelectorAll(".crew-row")); }
    // what the typed-in cuts leave, split equally among the untouched new members and whoever sells
    function even() {
      var typed = 0, auto = [];
      rows().forEach(function (r) {
        var a = r.querySelector("[name=crew_pct]");
        if (r.dataset.auto) auto.push(a); else typed += parseFloat(a.value) || 0;
      });
      if (!auto.length) return;
      var each = Math.floor(Math.max(0, 100 - typed) / (auto.length + 1) * 100) / 100;
      auto.forEach(function (a) { a.value = each; });
    }
    // a new row goes to the first member not aboard yet
    function pickCrew(r) {
      var taken = rows().filter(function (x) { return x !== r; }).map(function (x) { return x.querySelector("[name=crew_user]").value; });
      var sel = r.querySelector("[name=crew_user]");
      var free = Array.prototype.filter.call(sel.options, function (o) { return taken.indexOf(o.value) < 0; })[0];
      if (free) sel.value = free.value;
    }
    function wire(r) {
      r.querySelector("[name=crew_pct]").addEventListener("input", function () { delete r.dataset.auto; even(); });
      var rm = r.querySelector(".rm-crew");
      if (rm) rm.addEventListener("click", function () { raster([r], false, function () { r.remove(); even(); }); });
    }
    rows().forEach(wire);
    add.addEventListener("click", function () {
      var r = tpl.content.firstElementChild.cloneNode(true); box.appendChild(r);
      pickCrew(r); wire(r); even(); raster([r], true);
    });
  }
  document.addEventListener("htmx:load", function (e) {
    var root = e.detail && e.detail.elt; if (!root || !root.querySelectorAll) return;
    if (root.matches && root.matches("form.close-form")) wireClose(root);
    root.querySelectorAll("form.close-form").forEach(wireClose);
    if (root.matches && root.matches("form.crew-form")) wireCrew(root);
    root.querySelectorAll("form.crew-form").forEach(wireCrew);
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
          raster(ps.slice(3), true, function () { var h = document.getElementById("g-discord") || document.getElementById("g-handle"); if (h && gate) h.focus({ preventScroll: true }); armed = true; });
        }, t + 150));
      });
    }
    function dismiss() {
      if (!armed) return; armed = false;
      raster(parts(), false, function () { splash.hidden = true; });
    }
    // with a session, any key or click lets you through; without one, only the gate does
    ["click", "touchstart"].forEach(function (ev) {
      splash.addEventListener(ev, function (e) { if (mode0 !== "gate" && !e.target.closest(".cmd, button, a, input")) dismiss(); });
    });
    document.addEventListener("keydown", function (e) { if (!splash.hidden && mode0 !== "gate" && e.key !== "Tab") dismiss(); });
    var replay = document.getElementById("replay-splash");
    // blur first: a Space/Enter on the still-focused button would otherwise re-run the splash on keyup
    if (replay) replay.addEventListener("click", function (e) { e.stopPropagation(); replay.blur(); runSplash(mode0 === "gate"); });
    if (mode0 === "gate" || mode0 === "welcome") runSplash(mode0 === "gate");
    else splash.hidden = true;
  }

  /* ---- leaderboard meters ---- */
  document.querySelectorAll(".meter[data-pct]").forEach(function (m) { m.style.setProperty("--p", m.dataset.pct); });
})();
