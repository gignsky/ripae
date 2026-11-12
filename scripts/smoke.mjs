// Headless-browser smoke test for Ripae 0.2 (RIPAE-ROLL7.md §7).
//
//   node scripts/smoke.mjs            # from the repo root: spawns target/debug/ripae
//   RIPAE_URL=http://127.0.0.1:8080 node scripts/smoke.mjs   # against a running build
//
// Run it from the repo root: `ServeDir::new("static")` is cwd-relative, so
// a binary started elsewhere serves no stylesheet or script. Needs
// Playwright with Chromium (`npm i -g playwright` or a preinstalled one on
// NODE_PATH; `playwright` must resolve from this file or from NODE_PATH).
//
// Walks the whole 0.2 story: gate login → ledger → new run with two loads
// (one priced per lot) on a three-stop route, the second picked up at the
// first's drop → per-leg stowage plan → [NEXT STOP] → a second run → crate
// manifest: one crate lost, one stored (written off, but kept out of
// Realised while its voyage is open) → status reads SPLIT → put the other
// crew member on the voyage's crew on 25 %, forfeited back to the owner →
// close the first load selling two crates under that split → SALE banner
// shows the profit → leaderboard → a crew name → their ledger → service
// record → close the other load at a loss: the voyage closes, the
// write-off lands and Realised turns red → delete the
// second voyage through the confirmation → gone. Then asserts no
// horizontal scroll at 1280 / 760 / 400 px. Then the fleet: hulls (one
// carried in another, a pledge and its loaner), pull out / store / destroy /
// claim, delivered → loaners removed, a run flown by a hull that follows
// [NEXT STOP] on /fleet, and /fleet at the same widths. Then test mode: enter it, log
// a run, the TEST banner is up and another member still sees live, discard
// leaves live untouched; enter again, log a run, Keep shows the diff in
// words, confirm puts it in live. Motion is reduced so the run is
// deterministic; the raster itself has to be eyeballed. Opening / lands on
// the splash (Continue as ...) and the ledger's panels start folded, so the
// walk enters through it and hovers panels open before using them.

import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";

const require = createRequire(import.meta.url);
let chromium;
try {
  ({ chromium } = require("playwright"));
} catch {
  console.error("smoke: cannot resolve 'playwright' -- install it or put it on NODE_PATH");
  process.exit(2);
}

const GIG = { handle: "lordgig", name: "Lord Gig", pw: "smoke-gig" };
const JT = { handle: "firstofficer", name: "JT", pw: "smoke-jt" };

let server = null;
let tmp = null;
let base = process.env.RIPAE_URL;

async function startServer() {
  const bin = process.env.RIPAE_BIN || (existsSync("target/debug/ripae") ? "target/debug/ripae" : "target/release/ripae");
  if (!existsSync(bin)) throw new Error(`smoke: no binary at ${bin}; run cargo build first (or set RIPAE_URL)`);
  tmp = mkdtempSync(join(tmpdir(), "ripae-smoke-"));
  const bind = "127.0.0.1:18080";
  server = spawn(bin, [], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      RIPAE_BIND: bind,
      RIPAE_DB_PATH: join(tmp, "smoke.db"),
      RIPAE_SESSION_SECRET: randomBytes(64).toString("base64"),
      RIPAE_SEED_USERS: `${GIG.handle}:${GIG.name}:${GIG.pw},${JT.handle}:${JT.name}:${JT.pw}`,
      RIPAE_COOKIE_SECURE: "0",
      RUST_LOG: process.env.RUST_LOG || "warn",
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  base = `http://${bind}`;
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(base + "/healthz");
      if (r.ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("smoke: server did not answer /healthz");
}

function stopServer() {
  if (server) server.kill("SIGTERM");
  if (tmp) rmSync(tmp, { recursive: true, force: true });
}

const checks = [];
function check(name, ok, detail = "") {
  checks.push({ name, ok });
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? " -- " + detail : ""}`);
  if (!ok) throw new Error(`smoke: ${name}`);
}

async function fillLoadLine(line, f) {
  await line.locator("[name=commodity]").fill(f.commodity);
  await line.locator("[name=boxes]").fill(f.boxes);
  if (f.scu) {
    // SCU is generated from the boxes: read-only, never posted, no tab stop
    check(`SCU reads ${f.scu} from boxes ${f.boxes}`, (await line.locator("[data-scu]").textContent()).trim() === f.scu);
    check("SCU is not a form field", (await line.locator("[name=scu]").count()) === 0);
  }
  await line.locator("[name=price_mode]").selectOption(f.mode);
  if (f.mode === "lot") await line.locator("[name=price_lot]").fill(f.price);
  else await line.locator("[name=price_scu]").fill(f.price);
  // stops by number on the route: the pickup list starts at the start (0),
  // the drop list at "undecided" (0) then stop 1, 2, ...
  await line.locator("[name=pickup]").selectOption({ index: f.pickup });
  await line.locator("[name=drop]").selectOption({ index: f.drop });
}

// Opening the site with a session lands on the splash; [CONTINUE AS ...] lets you in.
async function enter(page) {
  await page.waitForSelector("#g-continue", { state: "visible" });
  await page.click("#g-continue");
  await page.waitForSelector("#splash", { state: "hidden" });
}

// The ledger's panels start folded; hovering a title bar opens it (and Open Cargo,
// Stowage Plan and Ledger stay open).
async function openPanel(page, id) {
  await page.hover(`#${id} > header .bar`);
  await page.waitForSelector(`#${id}.open > .body`, { state: "visible" });
}
async function openPanels(page) {
  for (const id of ["open-cargo-panel", "plans-panel", "ledger-panel"]) await openPanel(page, id);
}

const realisedVal = (page) => page.locator("#totals .tile").nth(1).locator(".val");
const realisedText = async (page) => (await realisedVal(page).innerText()).replace(/aUEC/, "").trim();

// route = [start, stop 1, stop 2, ...], built like a UEX route; `hull`
// picks a fleet hull by (part of) its label instead of typing a ship
async function newRun(page, ship, route, loads, crew, hull) {
  await page.goto(base + "/");
  await enter(page);
  await openPanel(page, "open-cargo-panel");
  await page.click("#newrun-toggle");
  await page.waitForSelector("#newrun.open #newrun-body", { state: "visible" });
  if (crew) await page.selectOption("#newrun-form [name=logged_by]", { label: crew });
  if (hull) {
    const value = await page.locator("#newrun-form [name=hull] option", { hasText: hull }).getAttribute("value");
    await page.selectOption("#newrun-form [name=hull]", value);
  } else await page.fill("#newrun-form [name=ship]", ship);
  const stops = page.locator("#newrun-form .route-stops > .route-stop");
  for (let i = 0; i < route.length; i++) {
    if ((await stops.count()) <= i) await page.click("#newrun-form [data-add-stop]");
    await stops.nth(i).locator("[name=stop_terminal]").fill(route[i]);
  }
  for (let i = 1; i < loads.length; i++) await page.click("#add-load");
  const lines = page.locator("#loads .cargo-line");
  for (let i = 0; i < loads.length; i++) await fillLoadLine(lines.nth(i), loads[i]);
  await Promise.all([page.waitForURL(base + "/"), page.click("#newrun-form button[type=submit]")]);
  await page.waitForSelector("#open-cargo", { state: "attached" });
  check("Open Cargo stays open across the post-redirect reload", await page.isVisible("#open-cargo-panel.open #open-cargo"));
  check("no splash after logging a run", await page.isHidden("#splash"));
  await openPanels(page);
}

async function noScroll(page, where) {
  for (const w of [1280, 760, 400]) {
    await page.setViewportSize({ width: w, height: 900 });
    await page.waitForTimeout(150);
    const [sw, cw] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
    check(`no horizontal scroll at ${w}px${where}`, sw === cw, `scrollWidth ${sw}, clientWidth ${cw}`);
  }
  await page.setViewportSize({ width: 1280, height: 900 });
}

const run = async () => {
  if (!base) await startServer();
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
  const page = await ctx.newPage();
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(e.message));
  // the wrong-password step is a deliberate 401; the browser logs it as a resource error
  page.on("console", (m) => { if (m.type() === "error" && !m.text().includes("401")) pageErrors.push(m.text()); });

  // -- gate --
  await page.goto(base + "/");
  check("unauthenticated / redirects to the gate", page.url().endsWith("/login"));
  await page.waitForSelector("#gate:not([hidden])");
  await page.fill("#g-handle", GIG.handle);
  await page.fill("#g-pass", "wrong");
  await page.click("#g-enter");
  await page.waitForSelector("#g-err");
  check("wrong pair: the door does not open", (await page.textContent("#g-err")).includes("The door does not open"));
  await page.fill("#g-handle", GIG.handle);
  await page.fill("#g-pass", GIG.pw);
  await Promise.all([page.waitForURL(base + "/"), page.click("#g-enter")]);
  await page.waitForSelector("#open-cargo", { state: "attached" });
  check("ledger visible after login", await page.isVisible("#open-cargo-panel"));
  check("rail names the crew member", (await page.textContent("#rail")).includes("LORD GIG"));
  check("no splash straight after signing in at the gate", await page.isHidden("#splash"));
  check("panels start folded to their title bar", !(await page.isVisible("#open-cargo")) && !(await page.isVisible("#newrun-form")) && !(await page.isVisible("#ledger")) && !(await page.isVisible("#plans")));
  check("title bars are disclosure buttons", (await page.getAttribute("#open-cargo-bar", "aria-expanded")) === "false" && (await page.getAttribute("#open-cargo-bar", "aria-controls")) === "open-cargo-body");

  // -- opening the site with a session lands on the splash --
  await page.goto(base + "/");
  await page.waitForSelector("#g-continue", { state: "visible" });
  check("landing on / shows the splash with Continue", (await page.textContent("#g-continue")).includes("Continue as LORD GIG"));
  await enter(page);
  await page.goto(base + "/board");
  check("a deep link skips the splash", await page.isHidden("#splash"));
  await page.click("a[data-go=ledger]");
  await page.waitForURL(base + "/");
  check("the rail's Ledger link skips the splash", await page.isHidden("#splash"));

  // -- hover intent: a pass over a bar doesn't open it --
  await page.hover("#plans-panel > header .bar");
  await page.mouse.move(5, 5);
  await page.waitForTimeout(350);
  check("a pointer passing over a bar leaves it folded", !(await page.isVisible("#plans")));

  // -- hover: Open Cargo stays open; New Run peeks, and stays only once clicked --
  await openPanel(page, "open-cargo-panel");
  await page.mouse.move(5, 5);
  check("Open Cargo stays open after the pointer leaves", await page.isVisible("#open-cargo"));
  check("its bar says expanded", (await page.getAttribute("#open-cargo-bar", "aria-expanded")) === "true");
  await page.hover("#newrun > header");
  await page.waitForSelector("#newrun-form", { state: "visible" });
  await page.mouse.move(5, 5);
  check("New Run lingers a moment after the pointer leaves", await page.isVisible("#newrun-form"));
  await page.waitForSelector("#newrun-form", { state: "hidden" });
  check("New Run folds again when the pointer leaves", true);
  await page.click("#newrun-toggle");
  await page.mouse.move(5, 5);
  check("New Run stays open once clicked", await page.isVisible("#newrun-form"));
  await page.click("#newrun .fold-hide");
  check("[HIDE] folds New Run", !(await page.isVisible("#newrun-form")));
  await page.click("#open-cargo-panel .fold-hide");
  check("[HIDE] folds Open Cargo", !(await page.isVisible("#open-cargo")));
  await page.focus("#ledger-bar");
  check("focusing a bar opens it", await page.isVisible("#ledger"));

  // -- new run with two loads on a three-stop route, one per lot: the
  //    Agricium is bought at Area18, where the Laranite is sold --
  await newRun(page, "Hull-C", ["ArcCorp Mining Area 141", "Area18 TDD", "Lorville TDD"], [
    { commodity: "Laranite", boxes: "3×8", mode: "scu", price: "2800", pickup: 0, drop: 1 },
    { commodity: "Agricium", boxes: "2x32", mode: "lot", price: "1612800", pickup: 1, drop: 2 },
  ]);
  const loadRows = page.locator("#open-cargo tbody.load");
  check("two loads on the voyage", (await loadRows.count()) === 2, `${await loadRows.count()} rows`);
  check("per-lot buy stored and shown", (await page.textContent("#open-cargo")).includes("1,612,800"));
  check("no table-wide header on Open Cargo", (await page.locator("#open-cargo thead").count()) === 0);
  const colhead = page.locator("#open-cargo tbody.vhead tr.colhead").first();
  const colText = (await colhead.textContent()).replace(/\s+/g, " ");
  check("each voyage carries its own column headers", colText.includes("Bought at / pickup") && colText.includes("Cost / reward"), colText);
  check("a new trade load waits at its buy terminal", (await loadRows.first().locator(".status").textContent()).trim() === "Awaiting pickup"
    && (await loadRows.first().locator("td.where").nth(1).textContent()).includes("awaiting pickup at ArcCorp Mining Area 141"));
  // [LOADED] on the Laranite, which boards at the start: every crate aboard in one click, no manifest
  const laraId = await loadRows.first().getAttribute("id");
  await page.locator(`#${laraId} .cmd`, { hasText: "Loaded" }).click();
  await page.waitForSelector(`#${laraId} .status.transit`);
  check("[LOADED] puts the load aboard", (await page.textContent(`#${laraId} td.where >> nth=1`)).includes("Hull-C") && !(await page.textContent(`#${laraId} td.where >> nth=1`)).includes("pickup")
    && (await page.locator(`#${laraId} .cmd`, { hasText: "Loaded" }).count()) === 0
    && (await page.locator(`#${laraId} tr.manifest`).count()) === 0, (await page.textContent(`#${laraId}`)).replace(/\s+/g, " "));
  check("stowage plan links sc-cargo.space for the hull", (await page.getAttribute("#plans a[href*='sc-cargo.space']", "href")).includes("hull-c-official"));
  check("the voyage header shows its route", (await page.textContent("#open-cargo tbody.vhead .vroute")).includes("Area18 TDD → Lorville TDD"));
  check("cargo picked up later waits for its stop", (await loadRows.nth(1).locator(".status").textContent()).trim() === "Awaiting pickup");
  const legs = page.locator("#plans .plan").first().locator(".leg");
  check("the plan runs leg by leg", (await legs.count()) === 2, `${await legs.count()} legs`);
  const leg1 = await legs.nth(0).textContent(), leg2 = await legs.nth(1).textContent();
  check("leg 1 carries only what boards at the start", leg1.includes("LARANITE") && !leg1.includes("AGRICIUM"), leg1.replace(/\s+/g, " "));
  check("leg 2 loads the Agricium at Area18", leg2.includes("AGRICIUM") && leg2.includes("loaded here") && leg2.includes("Off at Area18 TDD: LARANITE"));
  await page.locator("#open-cargo tbody.vhead .cmd", { hasText: "Next stop" }).click();
  await page.waitForSelector("#open-cargo tbody.load >> nth=1 >> .status.transit");
  check("[NEXT STOP] brings the Agricium aboard", (await loadRows.nth(1).locator("td.where >> nth=1").textContent()).includes("1 stop to go"));
  check("the header moves along the route", (await page.textContent("#open-cargo tbody.vhead .vroute .here")).includes("Area18"));
  // the route editor comes up with the voyage's stops and loads
  await page.locator("#open-cargo tbody.vhead .cmd", { hasText: "Edit" }).first().click();
  await page.waitForSelector("#open-cargo tbody.vhead form .route-stops");
  check("edit voyage shows the route", (await page.locator("#open-cargo tbody.vhead form .route-stop").count()) === 3);
  check("edit voyage assigns each open load", (await page.locator("#open-cargo tbody.vhead form [name=assign_pickup]").count()) === 2);
  await page.locator("#open-cargo tbody.vhead form .cmd", { hasText: "Cancel" }).click();
  await page.waitForSelector("#open-cargo tbody.vhead form", { state: "detached" });

  // -- a second run, no hull, kept for JT (JT buys and sells; Gig only does the books) --
  await newRun(page, "", ["Shubin Mining Facility SCD-1", "Area18 TDD"], [
    { commodity: "Titanium", boxes: "6x8", mode: "scu", price: "7600", pickup: 0, drop: 1 },
  ], "JT");
  check("second voyage listed", (await page.locator("#open-cargo tbody.vhead").count()) === 2);
  const jtHead = page.locator("#open-cargo tbody.vhead").nth(1);
  check("run kept for JT is logged by its bookkeeper, Gig", (await jtHead.textContent()).includes("Logged by LORD GIG"));
  const jtLoad = page.locator("#open-cargo tbody.load").nth(2);
  check("its load says JT bought it", (await jtLoad.locator("td.who").textContent()).trim() === "JT");

  // -- keyboard only: add a load to JT's voyage, Tab from field to field, Enter submits --
  const jtHeadId = await jtHead.getAttribute("id");
  const focused = () => page.evaluate(() => { const a = document.activeElement; return a ? a.getAttribute("name") || a.textContent.trim() : ""; });
  await page.locator(`#${jtHeadId} .cmd`, { hasText: "Add load" }).focus();
  await page.keyboard.press("Enter");
  await page.waitForSelector(`#${jtHeadId} form`);
  await page.waitForFunction(() => document.activeElement && document.activeElement.name === "kind");
  check("add-load form opens on its first field", (await focused()) === "kind");
  const order = [];
  const tab = async (text) => { await page.keyboard.press("Tab"); order.push(await focused()); if (text) await page.keyboard.type(text); };
  await tab("Gold");
  await tab("2x8");
  check("SCU follows the boxes as they are typed", (await page.textContent(`#${jtHeadId} [data-scu]`)).trim() === "16");
  await tab();
  await tab("5000");
  // pickup, drop, buyer and status keep their defaults: the start, the last
  // stop, the one logging it, awaiting pickup (stored-at shows only when stored)
  await tab();
  await tab();
  await tab();
  await tab();
  await tab("keyboard only"); // notes, a text field: Enter submits from here
  check("tab order follows the form, skipping the generated SCU", order.join(",") === "commodity,boxes,price_mode,price_scu,pickup,drop,bought_by,status,notes", order.join(","));
  const before = await page.locator("#open-cargo tbody.load").count();
  await page.keyboard.press("Enter");
  await page.waitForFunction((n) => document.querySelectorAll("#open-cargo tbody.load").length === n + 1, before);
  const gold = page.locator(`#open-cargo tbody.load`, { hasText: "GOLD" });
  check("keyboard-entered load saved", (await gold.locator("td.num").first().textContent()).trim() === "16" && (await gold.textContent()).includes("80,000"));
  await page.waitForFunction((id) => { const a = document.activeElement; return a && a.closest("#" + id) && a.textContent.trim() === "Add load"; }, jtHeadId);
  check("focus returns to [ADD LOAD] after the save", true);
  // delete it again, so JT's run can close and count on the board
  const goldId = await gold.getAttribute("id");
  await gold.locator(".cmd", { hasText: "Edit" }).click();
  await page.locator(`#${goldId} .cmd`, { hasText: "Del load" }).click();
  await page.waitForSelector("#overlay .cmd:has-text('Delete load')");
  await page.locator("#overlay .cmd", { hasText: "Delete load" }).click();
  await page.waitForSelector(`#${goldId}`, { state: "detached" });

  // -- crate manifest on the first load: one lost, one stored → SPLIT --
  const first = page.locator("#open-cargo tbody.load").first();
  const firstId = await first.getAttribute("id");
  await first.locator(".cmd", { hasText: "Crates" }).click();
  await page.waitForSelector(`#${firstId} tr.manifest`);
  check("crate manifest shows three crates", (await page.locator(`#${firstId} .crate-tile`).count()) === 3);
  await page.locator(`#${firstId} .crate-tile .crate`).first().click(); // sound → glitched
  await page.waitForSelector(`#${firstId} .crate.glitched`);
  await page.locator(`#${firstId} .crate-tile .crate`).first().click(); // glitched → lost
  await page.waitForSelector(`#${firstId} .crate.lost`);
  check("one crate lost, SCU written off", (await page.textContent(`#${firstId} .crates-h`)).includes("written off"));
  // 8 SCU × 2800 = 22,400 written off, but the voyage is still open: not yet off Realised
  check("open-voyage write-off stays out of Realised", (await realisedText(page)) === "0", await realisedText(page));
  await page.fill(`#stored-at-${firstId.replace("load-", "")}`, "Everus Harbor");
  await page.locator(`#${firstId} .crate-tile .wh`).nth(1).click(); // second crate: aboard → stored
  await page.waitForSelector(`#${firstId} .status.split`);
  check("status reads SPLIT", (await page.textContent(`#${firstId} .status`)).trim() === "Split");
  check("where-now lists both groups", (await page.textContent(`#${firstId} td.where >> nth=1`)).includes("stored at Everus Harbor"));

  // -- the voyage's crew: JT aboard on 25 % of every sale, forfeited to Gig --
  const firstHead = await page.locator("#open-cargo tbody.vhead").first().getAttribute("id");
  await page.locator(`#${firstHead} .cmd`, { hasText: "Edit" }).click();
  await page.waitForSelector(`#${firstHead} form.crew-form`);
  check("the owner is on the crew from the start", (await page.locator(`#${firstHead} .crew-row`).count()) === 1);
  await page.click(`#${firstHead} .add-crew`);
  const jtRow = page.locator(`#${firstHead} .crew-rows .crew-row`).nth(1);
  await jtRow.locator("[name=crew_user]").selectOption({ label: "JT" });
  await jtRow.locator("[name=crew_pct]").fill("25");
  await jtRow.locator("[name=crew_paid_to]").selectOption({ label: "LORD GIG" });
  await page.click(`#${firstHead} form.crew-form button[type=submit]`);
  await page.waitForSelector(`#${firstHead} .crew-line`);
  await page.waitForFunction((id) => !document.querySelector(`#${id} form.crew-form`), firstHead);
  check("voyage header lists the crew and the split", (await page.textContent(`#${firstHead} .crew-line`)).includes("JT 25% → LORD GIG"));

  // -- close the first load: two crates, split by the crew --
  await page.locator(`#${firstId} .cmd`, { hasText: "Close" }).click();
  await page.waitForSelector(`#${firstId} form.close-form`);
  const picks = page.locator(`#${firstId} label.crate input`);
  check("lost crate can't be picked", await picks.nth(0).isDisabled());
  check("two sound crates pre-selected", (await picks.nth(1).isChecked()) && (await picks.nth(2).isChecked()));
  await page.fill(`#${firstId} [name=sell_terminal]`, "Area18 TDD");
  await page.fill(`#${firstId} [name=sell_scu]`, "3600");
  const split = await page.textContent(`#${firstId} .crew-split`);
  check("close form shows the crew's split", split.includes("JT") && split.includes("25% of profit") && split.includes("forfeited to LORD GIG"));
  check("no per-sale share editor any more", (await page.locator(`#${firstId} [name=share_to], #${firstId} .add-share`).count()) === 0);
  const preview = await page.textContent(`#${firstId} .preview`);
  check("live preview computes the profit and the crew's cut", preview.includes("+12,800") && preview.includes("shares 3,200") && preview.includes("9,600"), preview);
  await page.click(`#${firstId} form.close-form button[type=submit]`);
  await page.waitForSelector("#overlay:not([hidden]) #sale-profit");
  const banner = await page.textContent("#overlay");
  check("SALE banner shows the profit", banner.includes("+12,800"), banner.replace(/\s+/g, " ").slice(0, 120));
  check("SALE banner lists the forfeited share", banner.includes("JT") && banner.includes("forfeited to LORD GIG"));
  check("SALE banner says what the seller keeps", banner.includes("9,600"));
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });
  check("ledger gained the sale", (await page.textContent("#ledger")).includes("+12,800"));
  check("panels stay open through htmx swaps", (await page.isVisible("#open-cargo")) && (await page.isVisible("#ledger")) && (await page.isVisible("#plans")));
  check("first load left Open Cargo (lost crate rides along)", (await page.locator(`#${firstId}`).count()) === 0);

  // -- the sale editor re-pays the sale's own shares unless asked to re-split by the crew --
  await page.locator("#ledger .cmd", { hasText: "Sale" }).first().click();
  await page.waitForSelector("#overlay:not([hidden]) [data-overlay=sale-edit]");
  const rec = await page.textContent("#overlay .split-lines.recorded");
  check("sale editor shows the shares as recorded", rec.includes("JT") && rec.includes("as recorded"));
  check("sale editor offers a re-split by the crew", (await page.locator("#overlay [name=resplit]").count()) === 1);
  await page.click("#overlay form.sale-edit button[type=submit]");
  await page.waitForSelector("#overlay", { state: "hidden" });
  check("saving the sale unchanged keeps its split", (await page.textContent("#ledger")).includes("→ JT 3,200 (to LORD GIG)"));

  // -- sell JT's load on JT's behalf: the close form defaults to the buyer --
  const jtId = await page.locator("#open-cargo tbody.load").nth(1).getAttribute("id");
  await page.locator(`#${jtId} .cmd`, { hasText: "Close" }).click();
  await page.waitForSelector(`#${jtId} form.close-form`);
  check("sold-by defaults to the load's buyer", (await page.locator(`#${jtId} [name=sold_by] option:checked`).textContent()) === "JT");
  await page.fill(`#${jtId} [name=sell_terminal]`, "Area18 TDD");
  await page.fill(`#${jtId} [name=sell_scu]`, "9000"); // 48 SCU × (9000 − 7600) = +67,200
  await page.click(`#${jtId} form.close-form button[type=submit]`);
  await page.waitForSelector("#overlay:not([hidden]) #sale-profit");
  const jtBanner = (await page.textContent("#overlay")).replace(/\s+/g, " ");
  check("SALE banner congratulates JT and names the bookkeeper", jtBanner.includes("SALE, JT") && jtBanner.includes("BOOKED BY LORD GIG"));
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });

  // -- leaderboard and service record --
  await page.goto(base + "/board");
  await page.waitForSelector("#board");
  check("leaderboard ranks JT first (his 67,200 beats Gig's 9,600 + 3,200)", (await page.textContent("#board tbody tr >> nth=0")).includes("JT"));
  check("JT's row carries his profit", (await page.textContent("#board tbody tr >> nth=0")).includes("+70,400"));
  check("recent shares shows the forfeit", (await page.textContent("#view-board")).includes("JT → LORD GIG"));
  const jtBoardRow = () => page.locator("#board tbody tr", { hasText: "firstofficer" });
  check("collected is what JT's sale took in, before cargo costs (48 × 9,000)", (await jtBoardRow().locator("td.collected").textContent()).includes("+432,000"));
  check("JT's row shows what he forfeited", (await jtBoardRow().locator("td.collected .forfeit").textContent()).includes("−3,200 forfeited"));
  check("Gig's row shows what was forfeited to him", (await page.locator("#board tbody tr", { hasText: "lordgig" }).locator(".forfeit-in").textContent()).includes("+3,200 from forfeits"));
  check("Profit and Collected headers explain themselves", (await page.getAttribute("#board th:has-text('Collected')", "title")).includes("Forfeited shares go to someone else"));
  check("by profit: the bar is the share of org profit", (await page.textContent("#board th.bar")).trim() === "Share of org profit");
  check("by profit: active sort is marked", (await page.getAttribute("#board th.sorted", "aria-sort")) === "descending" && (await page.textContent("#board th.sorted")).includes("Profit"));
  // the bar's label stays inside its cell and the panel at every width, on every sort
  const barsFit = () => page.evaluate(() => {
    const panel = document.querySelector("#board").closest(".panel").getBoundingClientRect();
    return Array.from(document.querySelectorAll("#board td.bar")).every((td) => {
      const c = td.getBoundingClientRect();
      const l = td.querySelector(".pct").getBoundingClientRect();
      return l.width > 0 && l.left >= c.left - 0.5 && l.right <= c.right + 0.5 && l.right <= panel.right - 0.5;
    });
  });
  for (const [sort, head] of [["runs", "Share of org runs"], ["scu", "Share of org SCU hauled"], ["collected", "Share of org collected"], ["profit", "Share of org profit"]]) {
    await page.locator(".sortby a.cmd", { hasText: `By ${sort === "scu" ? "SCU" : sort}` }).click();
    await page.waitForURL(`**/board?sort=${sort}`);
    await page.waitForSelector("#board");
    check(`by ${sort}: bar header follows the sort`, (await page.textContent("#board th.bar")).trim() === head);
    check(`by ${sort}: its sort command is lit`, (await page.getAttribute(".sortby a.on", "href")) === `/board?sort=${sort}` && (await page.getAttribute(".sortby a.on", "aria-current")) === "true");
    if (sort === "runs") check("by runs: someone on every closed run reads 100%", (await page.textContent("#board tbody tr >> nth=0")).includes("100%"), (await page.textContent("#board tbody")).replace(/\s+/g, " "));
    for (const w of [1280, 760, 400]) {
      await page.setViewportSize({ width: w, height: 900 });
      await page.waitForTimeout(100);
      const [sw, cw] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
      check(`board by ${sort}: no horizontal scroll at ${w}px`, sw === cw, `scrollWidth ${sw}, clientWidth ${cw}`);
      check(`board by ${sort}: share labels stay inside their cells at ${w}px`, await barsFit());
    }
    await page.setViewportSize({ width: 1280, height: 900 });
  }

  // -- a crew name opens their ledger; its totals are the board's --
  const gigRow = page.locator("#board tbody tr", { hasText: "Lord Gig" });
  const gigProfit = (await gigRow.locator("td.num").first().textContent()).trim();
  const gigCollected = (await gigRow.locator("td.collected").evaluate((td) => td.firstChild.textContent)).trim();
  await Promise.all([page.waitForURL(/\/crew\/\d+\/ledger$/), gigRow.locator(".crew").click()]);
  await page.waitForSelector("#member-ledger");
  check("member ledger totals match the leaderboard row", (await page.textContent("#ledger-credit")).trim() === gigProfit, `${gigProfit} vs ${await page.textContent("#ledger-credit")}`);
  check("the ledger's collected is the board's Collected", (await page.textContent("#ledger-cash")).includes(gigCollected), `${gigCollected} vs ${await page.textContent("#ledger-cash")}`);
  const lv = page.locator("#member-ledger tbody.lvoyage");
  check("one voyage on Gig's ledger, with his roles", (await lv.count()) === 1 && (await lv.first().textContent()).includes("logged · sold"));
  await lv.first().locator("[data-expand]").click();
  await page.waitForSelector("#member-ledger tr.lloads:not([hidden])");
  check("a voyage row opens to its loads", (await page.textContent("#member-ledger tr.lloads")).includes("LARANITE"));
  await noScroll(page, " on the member ledger");
  await page.locator("#crew-ledger .cmd", { hasText: "Service record" }).click();
  await page.waitForSelector("#overlay:not([hidden]) .record");
  check("service record visible", await page.isVisible("#overlay .record"));
  check("WOMP WOMP for rank 2", await page.isVisible("#overlay .ascii.womp"));
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });

  // -- close the Agricium load at a loss: its voyage closes, the 22,400 write-off lands --
  await page.goto(base + "/");
  await enter(page);
  await openPanels(page);
  check("Realised before the voyage closes is both sales, no write-off", (await realisedText(page)) === "80,000", await realisedText(page));
  const agId = await page.locator("#open-cargo tbody.load").first().getAttribute("id");
  await page.locator(`#${agId} .cmd`, { hasText: "Close" }).click();
  await page.waitForSelector(`#${agId} form.close-form`);
  await page.fill(`#${agId} [name=sell_terminal]`, "Lorville TDD");
  await page.fill(`#${agId} [name=sell_scu]`, "20000"); // 64 × 20000 − 1,612,800 = −332,800
  await page.click(`#${agId} form.close-form button[type=submit]`);
  await page.waitForSelector("#overlay:not([hidden]) #sale-profit");
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });
  // 12,800 + 67,200 − 332,800 − 22,400
  check("write-off lands once its voyage closes", (await realisedText(page)) === "-275,200", await realisedText(page));
  const negColor = await realisedVal(page).evaluate((e) => getComputedStyle(e).color);
  const committedColor = await page.locator("#totals .tile").nth(0).locator(".val").evaluate((e) => getComputedStyle(e).color);
  check("a net-loss Realised is red", (await realisedVal(page).getAttribute("class")).includes("neg") && negColor !== committedColor, negColor);

  // -- delete the remaining hull-less voyage through the confirmation --
  await newRun(page, "", ["Levski", "Grim HEX"], [
    { commodity: "Gold", boxes: "2x4", mode: "scu", price: "5000", pickup: 0, drop: 1 },
  ]);
  const heads = page.locator("#open-cargo tbody.vhead");
  const secondHead = await heads.last().getAttribute("id");
  await page.locator(`#${secondHead} .cmd`, { hasText: "Del" }).click();
  await page.waitForSelector("#overlay:not([hidden]) [data-overlay=confirm]");
  check("DELETE confirmation names the voyage", (await page.textContent("#overlay .tag")).includes("Delete"));
  await page.locator("#overlay .cmd", { hasText: "Keep it" }).click();
  await page.waitForSelector("#overlay", { state: "hidden" });
  check("[KEEP IT] keeps it", (await page.locator(`#${secondHead}`).count()) === 1);
  await page.locator(`#${secondHead} .cmd`, { hasText: "Del" }).click();
  await page.waitForSelector("#overlay:not([hidden]) [data-overlay=confirm]");
  await page.locator("#overlay .cmd", { hasText: "Delete voyage" }).click();
  await page.waitForSelector(`#${secondHead}`, { state: "detached" });
  check("[DELETE VOYAGE] removes it", (await page.locator(`#${secondHead}`).count()) === 0);

  // -- no horizontal scroll at any width --
  await noScroll(page, "");
  // -- and the screen fills a wide window rather than stopping at a fixed width --
  await page.setViewportSize({ width: 1920, height: 900 });
  await page.waitForTimeout(150);
  const fill = await page.evaluate(() => [document.querySelector(".screen").getBoundingClientRect().width, document.documentElement.clientWidth]);
  check("the screen fills a 1920px window", fill[0] >= fill[1] - 2 * Math.max(16, fill[1] * 0.015) - 1, `${fill[0]} of ${fill[1]}`);
  await page.setViewportSize({ width: 1280, height: 900 });

  // -- the fleet: hulls, a carried one, a pledge and its loaner --
  await page.goto(base + "/");
  check("the splash counts the fleet and puts UEX import in 0.5", await page.evaluate(() => {
    const b = document.getElementById("splash").dataset.boot;
    return b.includes("FLEET") && b.includes("0 HULLS") && b.includes("UEX IMPORT 0.5");
  }));
  await enter(page);
  await Promise.all([page.waitForURL(base + "/fleet"), page.click("#rail a[href='/fleet']")]);
  await page.waitForSelector("#fleet");
  check("the fleet tab is lit", (await page.getAttribute("#rail a[href='/fleet']", "class")).includes("on"));
  check("an empty fleet says so", (await page.textContent("#fleet")).includes("No hulls yet"));
  const row = (text) => page.locator("#fleet tr.hull", { has: page.locator(".hull-edit", { hasText: text }) });
  async function addHull(f) {
    await page.click("#add-hull");
    await page.waitForSelector("#overlay:not([hidden]) #hull-form");
    if (f.owner) await page.selectOption("#hull-form [name=owner]", { label: f.owner });
    await page.fill("#hull-form [name=model]", f.model);
    if (f.nickname) await page.fill("#hull-form [name=nickname]", f.nickname);
    if (f.kind) await page.selectOption("#hull-form [name=kind]", f.kind);
    if (f.status) await page.selectOption("#hull-form [name=status]", f.status);
    if (f.location) await page.fill("#hull-form [name=location]", f.location);
    if (f.carried) await page.selectOption("#hull-form [name=carried_in]", { label: f.carried });
    if (f.loanerFor) {
      await page.check("#hull-form [name=loaner]");
      await page.selectOption("#hull-form [name=loaner_for]", { label: f.loanerFor });
    }
    if (f.claim) await page.fill("#hull-form [name=insurance_claim]", f.claim);
    if (f.insurance) await page.fill("#hull-form [name=insurance_type]", f.insurance);
    await page.click("#hull-form button[type=submit]");
    await page.waitForSelector("#overlay", { state: "hidden" });
    await row(f.nickname || f.model).first().waitFor();
  }
  await addHull({ model: "ironclad", nickname: "Big Iron", location: "Area18 TDD", claim: "120,000", insurance: "LTI" });
  check("a hull from the list takes its spelling and capacity", (await row("Big Iron").textContent()).includes("Ironclad · Drake · Ship · 2,204 SCU"));
  await addHull({ model: "C8 Pisces", carried: "LORD GIG · Big Iron (Ironclad)", claim: "5000" });
  await addHull({ model: "Ursa Rover", kind: "vehicle", location: "Lorville TDD", owner: "JT" });
  check("an off-list model is kept as typed", (await row("Ursa Rover").textContent()).includes("Ground vehicle"));
  await addHull({ model: "Hull-D [Concept]", status: "planned" });
  await addHull({ model: "Hull-C", location: "Lorville TDD", loanerFor: "LORD GIG · Hull-D [Concept]" });
  check("the loaner is tagged and linked to its pledge", (await row("Hull-C").first().textContent()).includes("Loaner") && (await row("Hull-C").first().textContent()).includes("for Hull-D"));
  check("the pledge lists its loaner", (await row("Hull-D").textContent()).includes("Loaners: Hull-C"));
  const pisces = row("C8 Pisces");
  check("the carried hull nests under its carrier", (await pisces.getAttribute("class")).includes("carried"));
  check("the carried hull is where its carrier is", (await pisces.locator("td.where").first().textContent()).includes("in Big Iron (Ironclad) · Area18 TDD"));
  check("at a glance: hulls per status", (await page.textContent("#fleet-glance [data-status=stored]")).includes("4"));

  // pull out, store (the Pisces goes along), destroy, claim
  const act = async (r, label, location) => {
    if (location !== undefined) {
      const box = r.locator("form.hull-act [name=location]").first();
      await box.fill(location);
      await box.press("Escape"); // close the pick-list over the buttons
    }
    await Promise.all([
      page.waitForResponse((res) => res.url().includes("/hulls/") && res.request().method() === "POST"),
      r.locator("form.hull-act .cmd", { hasText: label }).first().click(),
    ]);
    await page.waitForTimeout(150); // the swap
  };
  await act(row("Big Iron"), "Pull out", "Area18 TDD");
  check("pulled out", (await row("Big Iron").locator(".status").first().textContent()).trim() === "Pulled out");
  check("its cargo hull is out with it", (await pisces.locator(".status").first().textContent()).trim() === "Pulled out");
  await act(row("Big Iron"), "Store", "Everus Harbor");
  check("stored where it was put", (await row("Big Iron").locator("td.where").first().textContent()).trim() === "Everus Harbor");
  check("the Pisces moved with it", (await pisces.locator("td.where").first().textContent()).includes("· Everus Harbor"));
  await act(pisces, "Pull out", "Everus Harbor");
  check("pulling out a carried hull unloads it", !(await pisces.getAttribute("class")).includes("carried"));
  await act(pisces, "Destroyed");
  check("a destroyed hull shows its claim cost", (await pisces.textContent()).includes("claim 5,000 aUEC"));
  check("at a glance: claims pending", (await page.textContent("#fleet-glance .fleet-claims")).includes("5,000"));
  await act(pisces, "Claim", "Levski");
  check("claimed: stored at the claim location", (await pisces.locator(".status").first().textContent()).trim() === "Stored" && (await pisces.locator("td.where").first().textContent()).trim() === "Levski");
  await act(row("Hull-D"), "Delivered", "Area18 TDD");
  check("delivering the pledge offers its loaners back", await row("Hull-D").locator(".loaners-back").isVisible());
  await row("Hull-D").locator(".loaners-back .cmd", { hasText: "Remove loaners" }).click();
  await page.waitForSelector("#fleet tr.hull:has-text('Loaner')", { state: "detached" });
  check("the loaner went back", (await row("Hull-C").count()) === 0);
  // the filter: JT's hulls only
  await page.goto(base + "/fleet?owner=2&sort=location");
  check("the owner filter shows one member's hulls", (await page.locator("#fleet tr.hull").count()) === 1 && (await page.textContent("#fleet")).includes("Ursa Rover"));

  // -- a run flown by Big Iron: pulled out at the start, follows Next stop --
  await newRun(page, "", ["Everus Harbor", "Area18 TDD", "Lorville TDD"], [
    { commodity: "Quartz", boxes: "2x8", mode: "scu", price: "300", pickup: 0, drop: 2 },
  ], null, "Big Iron");
  const ironHead = page.locator("#open-cargo tbody.vhead", { hasText: "Big Iron" });
  check("the voyage header names the hull", (await ironHead.count()) === 1);
  check("the run takes the hull's model", (await page.textContent("#plans")).includes("ironclad-official") || (await page.locator("#plans a[href*='ironclad']").count()) > 0);
  await page.goto(base + "/fleet");
  check("the hull is pulled out at the route's start", (await row("Big Iron").locator("td.where").first().textContent()).trim() === "Everus Harbor" && (await row("Big Iron").textContent()).includes("Flying Voyage"));
  await page.goto(base + "/");
  await enter(page);
  await openPanel(page, "open-cargo-panel");
  await ironHead.locator(".cmd", { hasText: "Next stop" }).click();
  await page.waitForSelector("#open-cargo tbody.vhead:has-text('Big Iron') .vroute .here:has-text('Area18')");
  await page.goto(base + "/fleet");
  check("[NEXT STOP] moves the hull on the fleet page", (await row("Big Iron").locator("td.where").first().textContent()).trim() === "Area18 TDD");
  check("the voyage it flies says where it is", (await row("Big Iron").textContent()).includes("at Area18 TDD · next Lorville TDD"));
  await noScroll(page, " on /fleet");

  // -- test mode: a sandbox for this session only; discard, then keep --
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(base + "/");
  await enter(page);
  await openPanels(page);
  const liveHeads = await page.locator("#open-cargo tbody.vhead").count();
  const sandboxRoute = ["Levski", "Grim HEX"];
  const sandboxRun = [{ commodity: "Quartz", boxes: "2x16", mode: "scu", price: "300", pickup: 0, drop: 1 }];
  check("no TEST banner on live", (await page.locator("#test-banner").count()) === 0);
  await Promise.all([page.waitForURL(base + "/"), page.click("#test-enter")]);
  check("TEST banner up in test mode", await page.isVisible("#test-banner"));
  await newRun(page, "Caterpillar", sandboxRoute, sandboxRun);
  check("the sandbox run shows in test mode", (await page.locator("#open-cargo tbody.vhead").count()) === liveHeads + 1);
  check("TEST banner stays up after a write", await page.isVisible("#test-banner"));
  // another member, on live meanwhile, sees none of it
  const jtCtx = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
  const jtPage = await jtCtx.newPage();
  await jtPage.goto(base + "/login");
  await jtPage.fill("#g-handle", JT.handle);
  await jtPage.fill("#g-pass", JT.pw);
  await Promise.all([jtPage.waitForURL(base + "/"), jtPage.click("#g-enter")]);
  await jtPage.waitForSelector("#open-cargo", { state: "attached" });
  check("other members stay on live", (await jtPage.locator("#open-cargo tbody.vhead").count()) === liveHeads && (await jtPage.locator("#test-banner").count()) === 0);
  await page.setViewportSize({ width: 400, height: 900 });
  await page.waitForTimeout(150);
  const [tsw, tcw] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
  check("TEST banner fits at 400px", tsw === tcw && (await page.isVisible("#test-discard")), `scrollWidth ${tsw}, clientWidth ${tcw}`);
  await page.setViewportSize({ width: 1280, height: 900 });
  await Promise.all([page.waitForURL(base + "/"), page.click("#test-discard")]);
  check("discard: back on live, banner gone", (await page.locator("#test-banner").count()) === 0);
  check("discard: live untouched", (await page.locator("#open-cargo tbody.vhead").count()) === liveHeads);

  await Promise.all([page.waitForURL(base + "/"), page.click("#test-enter")]);
  await newRun(page, "Caterpillar", sandboxRoute, sandboxRun);
  await Promise.all([page.waitForURL(base + "/test-mode/keep"), page.click("#test-keep")]);
  const diffText = (await page.textContent("#test-diff")).replace(/\s+/g, " ");
  check("keep shows the diff in words", diffText.includes("New voyage · Caterpillar") && diffText.includes("New load Quartz 32 SCU bought at Levski for 9,600 aUEC"), diffText.slice(0, 160));
  check("still in test mode while reviewing", await page.isVisible("#test-banner"));
  await Promise.all([page.waitForURL(base + "/"), page.click("#test-confirm")]);
  check("confirm: back on live, banner gone", (await page.locator("#test-banner").count()) === 0);
  const pushedHeads = await page.locator("#open-cargo tbody.vhead").count();
  check("confirm: the run is in live", pushedHeads === liveHeads + 1 && (await page.textContent("#open-cargo")).toUpperCase().includes("QUARTZ"), `${pushedHeads} voyages open, ${liveHeads} before`);
  await jtPage.goto(base + "/");
  check("other members see the pushed run", (await jtPage.textContent("#open-cargo")).toUpperCase().includes("QUARTZ"));
  await jtCtx.close();

  check("no page errors", pageErrors.length === 0, pageErrors.join(" | "));
  await browser.close();
};

try {
  await run();
  console.log(`smoke: ${checks.length} checks passed`);
  stopServer();
  process.exit(0);
} catch (e) {
  console.error(e.message || e);
  stopServer();
  process.exit(1);
}
