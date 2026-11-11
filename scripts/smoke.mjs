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
// (one priced per lot) → a second run → crate manifest: one crate lost, one
// stored (written off, but kept out of Realised while its voyage is open) →
// status reads SPLIT → close the first load selling two crates
// with a 25 % share forfeited to the other crew member → SALE banner shows
// the profit → leaderboard → a crew name → service record → close the other
// load at a loss: the voyage closes, the write-off lands and Realised turns
// red → delete the
// second voyage through the confirmation → gone. Then asserts no
// horizontal scroll at 1280 / 760 / 400 px. Then test mode: enter it, log
// a run, the TEST banner is up and another member still sees live, discard
// leaves live untouched; enter again, log a run, Keep shows the diff in
// words, confirm puts it in live. Motion is reduced so the run is
// deterministic; the raster itself has to be eyeballed.

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
  await line.locator("[name=price_mode]").selectOption(f.mode);
  if (f.mode === "lot") await line.locator("[name=price_lot]").fill(f.price);
  else await line.locator("[name=price_scu]").fill(f.price);
  await line.locator("[name=terminal]").fill(f.terminal);
  await line.locator("[name=stop_no]").fill(String(f.stop));
  await line.locator("[name=destination]").fill(f.dest);
}

const realisedVal = (page) => page.locator("#totals .tile").nth(1).locator(".val");
const realisedText = async (page) => (await realisedVal(page).innerText()).replace(/aUEC/, "").trim();

async function newRun(page, ship, loads, crew) {
  await page.goto(base + "/");
  await page.click("#newrun-toggle");
  await page.waitForSelector("#newrun-body:not([hidden])");
  if (crew) await page.selectOption("#newrun-form [name=logged_by]", { label: crew });
  await page.fill("#newrun-form [name=ship]", ship);
  for (let i = 1; i < loads.length; i++) await page.click("#add-load");
  const lines = page.locator("#loads .cargo-line");
  for (let i = 0; i < loads.length; i++) await fillLoadLine(lines.nth(i), loads[i]);
  await Promise.all([page.waitForURL(base + "/"), page.click("#newrun-form button[type=submit]")]);
  await page.waitForSelector("#open-cargo");
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
  await page.waitForSelector("#open-cargo");
  check("ledger visible after login", await page.isVisible("#open-cargo-panel"));
  check("rail names the crew member", (await page.textContent("#rail")).includes("LORD GIG"));

  // -- new run with two loads, one per lot --
  await newRun(page, "Hull-C", [
    { commodity: "Laranite", boxes: "3×8", mode: "scu", price: "2800", terminal: "ArcCorp Mining Area 141", stop: 1, dest: "Area18 TDD" },
    { commodity: "Agricium", boxes: "2x32", mode: "lot", price: "1612800", terminal: "Shubin Mining Facility SCD-1", stop: 2, dest: "Lorville TDD" },
  ]);
  const loadRows = page.locator("#open-cargo tbody.load");
  check("two loads on the voyage", (await loadRows.count()) === 2, `${await loadRows.count()} rows`);
  check("per-lot buy stored and shown", (await page.textContent("#open-cargo")).includes("1,612,800"));
  check("stowage plan links sc-cargo.space for the hull", (await page.getAttribute("#plans a[href*='sc-cargo.space']", "href")).includes("hull-c-official"));

  // -- a second run, no hull, kept for JT (JT buys and sells; Gig only does the books) --
  await newRun(page, "", [
    { commodity: "Titanium", boxes: "6x8", mode: "scu", price: "7600", terminal: "Shubin Mining Facility SCD-1", stop: 1, dest: "Area18 TDD" },
  ], "JT");
  check("second voyage listed", (await page.locator("#open-cargo tbody.vhead").count()) === 2);
  const jtHead = page.locator("#open-cargo tbody.vhead").nth(1);
  check("run kept for JT is logged by its bookkeeper, Gig", (await jtHead.textContent()).includes("Logged by LORD GIG"));
  const jtLoad = page.locator("#open-cargo tbody.load").nth(2);
  check("its load says JT bought it", (await jtLoad.locator("td.who").textContent()).trim() === "JT");

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

  // -- close the first load: two crates, 25 % share to JT forfeited to Gig --
  await page.locator(`#${firstId} .cmd`, { hasText: "Close" }).click();
  await page.waitForSelector(`#${firstId} form.close-form`);
  const picks = page.locator(`#${firstId} label.crate input`);
  check("lost crate can't be picked", await picks.nth(0).isDisabled());
  check("two sound crates pre-selected", (await picks.nth(1).isChecked()) && (await picks.nth(2).isChecked()));
  await page.fill(`#${firstId} [name=sell_terminal]`, "Area18 TDD");
  await page.fill(`#${firstId} [name=sell_scu]`, "3600");
  await page.click(`#${firstId} .add-share`);
  await page.waitForSelector(`#${firstId} .share`);
  await page.selectOption(`#${firstId} [name=share_to]`, { label: "JT" });
  await page.fill(`#${firstId} [name=share_amount]`, "25");
  await page.selectOption(`#${firstId} [name=share_paid_to]`, { label: "LORD GIG" });
  check("live preview computes the profit", (await page.textContent(`#${firstId} .preview`)).includes("+12,800"));
  await page.click(`#${firstId} form.close-form button[type=submit]`);
  await page.waitForSelector("#overlay:not([hidden]) #sale-profit");
  const banner = await page.textContent("#overlay");
  check("SALE banner shows the profit", banner.includes("+12,800"), banner.replace(/\s+/g, " ").slice(0, 120));
  check("SALE banner lists the forfeited share", banner.includes("JT") && banner.includes("forfeited to LORD GIG"));
  check("SALE banner says what the seller keeps", banner.includes("9,600"));
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });
  check("ledger gained the sale", (await page.textContent("#ledger")).includes("+12,800"));
  check("first load left Open Cargo (lost crate rides along)", (await page.locator(`#${firstId}`).count()) === 0);

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
  const jtRow = () => page.locator("#board tbody tr", { hasText: "firstofficer" });
  check("collected is what JT's sale took in, before cargo costs (48 × 9,000)", (await jtRow().locator("td.collected").textContent()).includes("+432,000"));
  check("JT's row shows what he forfeited", (await jtRow().locator("td.collected .forfeit").textContent()).includes("−3,200 forfeited"));
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
    if (sort === "runs") check("by runs: someone on every closed run reads 100%", (await page.textContent("#board tbody tr >> nth=0")).includes("100%"));
    for (const w of [1280, 760, 400]) {
      await page.setViewportSize({ width: w, height: 900 });
      await page.waitForTimeout(100);
      const [sw, cw] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
      check(`board by ${sort}: no horizontal scroll at ${w}px`, sw === cw, `scrollWidth ${sw}, clientWidth ${cw}`);
      check(`board by ${sort}: share labels stay inside their cells at ${w}px`, await barsFit());
    }
    await page.setViewportSize({ width: 1280, height: 900 });
  }
  await page.locator("#board .crew", { hasText: "Lord Gig" }).click();
  await page.waitForSelector("#overlay:not([hidden]) .record");
  check("service record visible", await page.isVisible("#overlay .record"));
  check("WOMP WOMP for rank 2", await page.isVisible("#overlay .ascii.womp"));
  await page.click("#overlay [data-dismiss]");
  await page.waitForSelector("#overlay", { state: "hidden" });

  // -- close the Agricium load at a loss: its voyage closes, the 22,400 write-off lands --
  await page.goto(base + "/");
  await page.waitForSelector("#open-cargo");
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
  await newRun(page, "", [
    { commodity: "Gold", boxes: "2x4", mode: "scu", price: "5000", terminal: "Levski", stop: 1, dest: "Grim HEX" },
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
  for (const w of [1280, 760, 400]) {
    await page.setViewportSize({ width: w, height: 900 });
    await page.waitForTimeout(150);
    const [sw, cw] = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
    check(`no horizontal scroll at ${w}px`, sw === cw, `scrollWidth ${sw}, clientWidth ${cw}`);
  }

  // -- test mode: a sandbox for this session only; discard, then keep --
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto(base + "/");
  const liveHeads = await page.locator("#open-cargo tbody.vhead").count();
  const sandboxRun = [{ commodity: "Quartz", boxes: "2x16", mode: "scu", price: "300", terminal: "Levski", stop: 1, dest: "Grim HEX" }];
  check("no TEST banner on live", (await page.locator("#test-banner").count()) === 0);
  await Promise.all([page.waitForURL(base + "/"), page.click("#test-enter")]);
  check("TEST banner up in test mode", await page.isVisible("#test-banner"));
  await newRun(page, "Caterpillar", sandboxRun);
  check("the sandbox run shows in test mode", (await page.locator("#open-cargo tbody.vhead").count()) === liveHeads + 1);
  check("TEST banner stays up after a write", await page.isVisible("#test-banner"));
  // another member, on live meanwhile, sees none of it
  const jtCtx = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
  const jtPage = await jtCtx.newPage();
  await jtPage.goto(base + "/login");
  await jtPage.fill("#g-handle", JT.handle);
  await jtPage.fill("#g-pass", JT.pw);
  await Promise.all([jtPage.waitForURL(base + "/"), jtPage.click("#g-enter")]);
  await jtPage.waitForSelector("#open-cargo");
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
  await newRun(page, "Caterpillar", sandboxRun);
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
