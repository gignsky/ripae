//! 0.2 "Stowage": voyages carrying cargo loads made of crates. `model` holds
//! the types and every derived figure; `repo` the SQL; `views` the
//! formatted shapes the templates read; this file the handlers.
//!
//! Fragments are Askama partials swapped by htmx. A mutating response
//! carries its primary fragment plus out-of-band copies of whatever else
//! the change touched (totals, stowage plan, a voyage header, the whole
//! Open Cargo table, the Ledger), so the page never goes stale without a
//! reload. Validation failures re-render the form with the *submitted*
//! values and the message, status 422.

pub mod model;
pub mod repo;
pub mod views;

use axum::extract::{Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::{is_landing, take_splash_mode, CurrentUser, SplashMode};
use crate::lookups::{self, Kind};
use crate::templates::{
    render_str, BoardTemplate, ConfirmFrag, ConfirmLine, ConfirmWriteOffFrag, SaleOverlayFrag, VoyageOverlayFrag, IndexTemplate, LedgerFrag, LoadFrag,
    OpenCargoFrag, PlansFrag, SaleFrag, Shell, TotalsFrag, VerdictFrag, VoyageHeadFrag,
};
use crate::sandbox::Db;
use crate::AppState;
use model::{
    parse_boxes, parse_date, service_record, resettle, settle, today, Load, LoadKind, PriceMode, Sale, SaleInput, ShareInput,
    ShareKind, SortKey, Totals, User, Voyage, Whereabouts,
};
use repo::{EditLoad, EditVoyage, MoveTarget, NewLoad, NewVoyage, RouteEdit};
use views::{
    board_view, ledger_rows, plan_cards, record_view, route_options, CloseFormView, LoadExtra, LoadFormView,
    LoadView, MergeChoice, NewRunFormView, RouteAssign, RouteFormView, RouteRow, ShareFormRow, ShareView,
    TotalsView, VoyageExtra, VoyageFormView, VoyageView,
};

// ---------------------------------------------------------------------------
// form bodies
// ---------------------------------------------------------------------------

/// `application/x-www-form-urlencoded` as submitted, repeated names kept in
/// order -- the multi-load New Run form and the shares rows rely on that.
pub struct FormData(Vec<(String, String)>);

impl FormData {
    pub fn parse(body: &str) -> Self {
        FormData(
            form_urlencoded::parse(body.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        )
    }

    pub fn get(&self, name: &str) -> &str {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }

    pub fn all(&self, name: &str) -> Vec<&str> {
        self.0.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect()
    }

    /// The nth value of a repeated field, or `""`.
    pub fn nth(&self, name: &str, i: usize) -> &str {
        self.all(name).get(i).copied().unwrap_or("")
    }

    pub fn count(&self, name: &str) -> usize {
        self.all(name).len()
    }
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

// ---------------------------------------------------------------------------
// the shell: rail + splash data for every page
// ---------------------------------------------------------------------------

/// Boot-log lines the splash types out. Open-cargo figures only once
/// identified; the roster count is harmless either way.
pub async fn shell(
    state: &AppState,
    user: Option<&CurrentUser>,
    screen: &'static str,
    mode: SplashMode,
    gate_error: Option<&'static str>,
) -> Shell {
    let crew: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    let cargo = match user {
        Some(_) => match repo::list_voyages(&state.pool).await {
            Ok(v) => {
                let t = Totals::from_voyages(&v);
                format!("{} LOAD{} · {} SCU", t.open_loads, if t.open_loads == 1 { "" } else { "S" }, t.open_scu)
            }
            Err(_) => "UNREADABLE".to_string(),
        },
        None => "RESTRICTED".to_string(),
    };
    let dev_logins = if state.dev_quick_login && user.is_none() {
        sqlx::query_as("SELECT id, handle, display_name FROM users ORDER BY id")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let lines = [
        format!("RIPAE INTERFACE v{} ........ ONLINE", crate::changelog::VERSION),
        "LEDGER STORE ................... MOUNTED".to_string(),
        format!("CREW ROSTER .................... {crew} ABOARD"),
        format!("OPEN CARGO ..................... {cargo}"),
        "PRICE FEED ..................... MANUAL (UEX IMPORT 0.4)".to_string(),
        String::new(),
    ];
    Shell {
        crew_name: user.map(|u| u.display_name.clone()),
        screen,
        stardate: today().to_string(),
        splash_mode: mode.as_str(),
        splash_who: user.map(|u| u.display_name.to_uppercase()).unwrap_or_default(),
        boot_json: serde_json::to_string(&lines).unwrap_or_else(|_| "[]".into()),
        gate_error,
        discord: state.discord.is_some(),
        test_mode: state.sandbox.is_some(),
        dev_logins,
    }
}

// ---------------------------------------------------------------------------
// shared plumbing
// ---------------------------------------------------------------------------

struct Ledger {
    voyages: Vec<Voyage>,
    users: Vec<User>,
}

async fn ledger(state: &AppState) -> Result<Ledger, Response> {
    let voyages = repo::list_voyages(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "failed to list voyages");
        (StatusCode::INTERNAL_SERVER_ERROR, "could not load the ledger").into_response()
    })?;
    let users = repo::list_users(&state.pool).await.map_err(|e| {
        tracing::error!(error = %e, "failed to list users");
        (StatusCode::INTERNAL_SERVER_ERROR, "could not load the crew").into_response()
    })?;
    Ok(Ledger { voyages, users })
}

fn html(status: StatusCode, body: String) -> Response {
    (status, axum::response::Html(body)).into_response()
}

/// A fragment response that lands somewhere other than the requesting
/// element's target (e.g. a form's success swaps the whole table).
fn retarget(mut resp: Response, selector: &str, swap: &str) -> Response {
    let h = resp.headers_mut();
    h.insert("HX-Retarget", HeaderValue::from_str(selector).expect("static selector"));
    h.insert("HX-Reswap", HeaderValue::from_str(swap).expect("static swap"));
    resp
}

fn totals_frag(l: &Ledger, oob: bool) -> String {
    render_str(&TotalsFrag { t: TotalsView::new(&Totals::from_voyages(&l.voyages)), oob })
}

fn plans_frag(l: &Ledger, oob: bool) -> String {
    render_str(&PlansFrag { cards: plan_cards(&l.voyages), oob })
}

fn open_voyages(l: &Ledger) -> Vec<VoyageView> {
    l.voyages
        .iter()
        .filter(|v| v.open_loads().next().is_some())
        .map(VoyageView::new)
        .collect()
}

fn open_cargo_frag(l: &Ledger, oob: bool) -> String {
    render_str(&OpenCargoFrag { voyages: open_voyages(l), oob, head_oob: false, load_oob: false })
}

fn ledger_frag(l: &Ledger, oob: bool) -> String {
    render_str(&LedgerFrag { rows: ledger_rows(&l.voyages), oob })
}

/// `GET /live/panels` -- every live panel of the ledger screen, refetched
/// when another crew member changes something (see `crate::live`).
pub async fn live_panels(Db(state): Db, _user: CurrentUser) -> Response {
    match ledger(&state).await {
        Ok(l) => html(
            StatusCode::OK,
            [totals_frag(&l, false), open_cargo_frag(&l, false), plans_frag(&l, false), ledger_frag(&l, false)].concat(),
        ),
        Err(r) => r,
    }
}

fn head_frag(v: VoyageView, head_oob: bool) -> String {
    render_str(&VoyageHeadFrag { v, head_oob })
}

fn load_frag(l: LoadView, load_oob: bool) -> String {
    render_str(&LoadFrag { l, load_oob })
}

/// Totals + stowage plan, out of band -- what nearly every change touches.
fn oob_common(l: &Ledger) -> String {
    format!("{}{}", totals_frag(l, true), plans_frag(l, true))
}

/// The Open Cargo table as the primary fragment, with totals and plan oob.
fn open_cargo_response(l: &Ledger) -> Response {
    html(StatusCode::OK, format!("{}{}", open_cargo_frag(l, false), oob_common(l)))
}

fn voyage_view(l: &Ledger, id: i64) -> Option<VoyageView> {
    l.voyages.iter().find(|v| v.id == id).map(VoyageView::new)
}

fn load_view(l: &Ledger, id: i64) -> Option<LoadView> {
    l.voyages.iter().find_map(|v| {
        v.loads
            .iter()
            .find(|x| x.id == id)
            .map(|x| LoadView { can_detach: v.open_loads().count() > 1, ..LoadView::new(x, v) })
    })
}

fn not_found(what: &str) -> Response {
    (StatusCode::NOT_FOUND, format!("No such {what}.")).into_response()
}

async fn find_load(state: &AppState, id: i64) -> Result<Load, Response> {
    match repo::find_load(&state.pool, id).await {
        Ok(Some(l)) => Ok(l),
        Ok(None) => Err(not_found("load")),
        Err(e) => {
            tracing::error!(error = %e, "failed to load load");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "could not load that load").into_response())
        }
    }
}

async fn find_voyage(state: &AppState, id: i64) -> Result<Voyage, Response> {
    match repo::find_voyage(&state.pool, id).await {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(not_found("voyage")),
        Err(e) => {
            tracing::error!(error = %e, "failed to load voyage");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "could not load that voyage").into_response())
        }
    }
}

async fn pick(state: &AppState, kind: Kind, raw: &str) -> String {
    lookups::canonical(&state.pool, kind, raw).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, list = kind.label(), "failed to check pick-list");
        raw.trim().to_string()
    })
}

async fn pick_opt(state: &AppState, kind: Kind, raw: &str) -> Option<String> {
    match opt(raw) {
        Some(v) => Some(pick(state, kind, &v).await),
        None => None,
    }
}

/// Resolves a typed hull name. Blank = no hull; an unknown name is refused
/// (a hull needs an sc-cargo.space slug, so ships come from the seeded list).
async fn pick_ship(state: &AppState, raw: &str) -> Result<Option<i64>, String> {
    let Some(name) = opt(raw) else { return Ok(None) };
    match repo::find_ship_by_name(&state.pool, &name).await {
        Ok(Some(s)) => Ok(Some(s.id)),
        Ok(None) => Err(format!(
            "Ship \"{name}\" isn't a known hull -- pick one from the list, or leave it blank until you have one."
        )),
        Err(e) => {
            tracing::error!(error = %e, "failed to look up ship");
            Err("Could not check the ship list.".into())
        }
    }
}

fn parse_whereabouts(raw: &str) -> Result<Option<Whereabouts>, String> {
    match raw.trim() {
        "" => Ok(None),
        "aboard" | "in_transit" => Ok(Some(Whereabouts::Aboard)),
        "stored" => Ok(Some(Whereabouts::Stored)),
        "awaiting_pickup" | "pickup" => Ok(Some(Whereabouts::AwaitingPickup)),
        other => Err(format!("\"{other}\" isn't a status.")),
    }
}

fn parse_money(raw: &str, what: &str) -> Result<f64, String> {
    let t = raw.trim().replace(',', "");
    if t.is_empty() {
        return Err(format!("{what} is required."));
    }
    let v: f64 = t.parse().map_err(|_| format!("{what}: \"{}\" isn't a number.", raw.trim()))?;
    if v < 0.0 || !v.is_finite() {
        return Err(format!("{what} can't be negative."));
    }
    Ok(v)
}

/// The game doesn't always show the real price, so a price can be entered as
/// the wallet balance before and after the trade. A purchase must lower the
/// balance and a sale raise it; the other way round is almost certainly the
/// two figures swapped, so it's refused rather than flipped.
fn balance_delta(before: &str, after: &str, selling: bool) -> Result<f64, String> {
    let b = parse_money(before, "Balance before")?;
    let a = parse_money(after, "Balance after")?;
    let delta = if selling { a - b } else { b - a };
    if delta < 0.0 {
        return Err(if selling {
            "The balance went down on a sale -- are before and after swapped?".into()
        } else {
            "The balance went up on a purchase -- are before and after swapped?".into()
        });
    }
    Ok(delta)
}

/// Everything a load line needs, validated and canonicalised. Shared by New
/// Run, Add Load and Edit.
struct LoadParts {
    kind: LoadKind,
    commodity: String,
    buy_terminal: String,
    price_mode: PriceMode,
    buy_price_scu: f64,
    buy_price_lot: f64,
    pickup_stop: u32,
    drop_stop: Option<u32>,
    destination: Option<String>,
    boxes: model::BoxMix,
    reward: Option<f64>,
    whereabouts: Option<Whereabouts>,
    stored_at: Option<String>,
    notes: Option<String>,
}

/// A route as validated: each stop's form key and canonical terminal,
/// start first. Loads pick their stops by key.
type Route = Vec<(String, String)>;

/// A route stop's number by its form key.
fn stop_index(route: &Route, key: &str) -> Option<u32> {
    route.iter().position(|(k, _)| k == key.trim()).map(|i| i as u32)
}

/// A voyage's stored route keyed by stop number -- what Add Load and Edit
/// Load pick from.
fn route_of(v: &Voyage) -> Route {
    let mut r: Route = v.route.iter().enumerate().map(|(i, t)| (i.to_string(), t.clone())).collect();
    if r.is_empty() {
        r.push(("0".into(), String::new()));
    }
    r
}

/// The route editor's rows as submitted (`stop_key` / `stop_terminal`, start first).
fn route_rows_from(form: &FormData) -> Vec<RouteRow> {
    let terminals = form.all("stop_terminal");
    form.all("stop_key")
        .iter()
        .enumerate()
        .map(|(i, k)| RouteRow { key: k.to_string(), terminal: terminals.get(i).copied().unwrap_or("").to_string() })
        .collect()
}

/// Validates the route rows: terminals snap to the pick-list's spelling; a
/// stop row left blank is dropped (a load still pointing at it is refused
/// by `parse_load_line`); only the start may be unnamed.
async fn parse_route(state: &AppState, rows: &[RouteRow]) -> Result<Route, String> {
    if rows.is_empty() {
        return Err("A voyage needs a route: where it starts, then its stops.".into());
    }
    let mut route = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        match opt(&r.terminal) {
            Some(t) => route.push((r.key.clone(), pick(state, Kind::Terminal, &t).await)),
            None if i == 0 => route.push((r.key.clone(), String::new())),
            None => {}
        }
    }
    Ok(route)
}

/// A load's pickup and drop stops by key: picked up at a stop on the route,
/// dropped at a later one (or undecided).
fn pick_stops(route: &Route, pickup: &str, drop: &str) -> Result<(u32, Option<u32>), String> {
    let p = match opt(pickup) {
        None => 0,
        Some(k) => stop_index(route, &k).ok_or("Its pickup isn't a stop on the route (a stop left blank?).")?,
    };
    let d = match opt(drop) {
        None => None,
        Some(k) => Some(stop_index(route, &k).ok_or("Its drop-off isn't a stop on the route (a stop left blank?).")?),
    };
    if d.is_some_and(|d| d <= p) {
        return Err("It has to come off at a stop after the one it's picked up at.".into());
    }
    Ok((p, d))
}

async fn parse_load_line(state: &AppState, f: &LoadFormView, boxes_editable: bool, existing_scu: u32, route: &Route) -> Result<LoadParts, String> {
    let commodity = opt(&f.commodity).ok_or("Commodity is required.")?;
    let (boxes, scu) = if boxes_editable {
        // Crates define the SCU: it is always the boxes' total. Any `scu`
        // field a client posts is ignored.
        let boxes = parse_boxes(&f.boxes).map_err(|e| format!("Boxes: {e}"))?;
        let total: u32 = boxes.iter().map(|(n, s)| n * s).sum();
        (boxes, total)
    } else {
        (Vec::new(), existing_scu)
    };
    let kind: LoadKind = f.kind.parse().unwrap_or_default();
    // A haul is picked up for nothing; what it earns is the contract reward,
    // paid on delivery. "balance" is a form-only mode: the drop in balance is
    // what the lot cost.
    let (price_mode, price, reward) = match (kind, f.price_mode.as_str()) {
        (LoadKind::Haul, _) => (PriceMode::Lot, 0.0, Some(parse_money(&f.reward, "Contract reward")?)),
        (LoadKind::Trade, "balance") => (PriceMode::Lot, balance_delta(&f.balance_before, &f.balance_after, false)?, None),
        (LoadKind::Trade, mode) => match mode.parse().unwrap_or(PriceMode::Scu) {
            PriceMode::Scu => (PriceMode::Scu, parse_money(&f.price_scu, "Buy price per SCU")?, None),
            PriceMode::Lot => (PriceMode::Lot, parse_money(&f.price_lot, "Price paid for the lot")?, None),
        },
    };
    // Bought where it's picked up. Free cargo (price 0, hauls included) may
    // board at an unnamed start; its buy terminal is stored as "".
    let (pickup_stop, drop_stop) = pick_stops(route, &f.pickup, &f.drop)?;
    let buy_terminal = route[pickup_stop as usize].1.clone();
    if buy_terminal.is_empty() && price != 0.0 {
        return Err("Name where it's bought: the route's start has no terminal (leave it blank only when the price is 0).".into());
    }
    let destination = drop_stop.map(|d| route[d as usize].1.clone());
    let (buy_price_scu, buy_price_lot) = model::derive_prices(price_mode, price, scu);
    let whereabouts = parse_whereabouts(&f.status)?;
    let stored_at = pick_opt(state, Kind::Terminal, &f.stored_at).await;
    if whereabouts == Some(Whereabouts::Stored) && stored_at.is_none() {
        return Err("Stored cargo needs a stored-at station.".into());
    }
    Ok(LoadParts {
        kind,
        commodity: pick(state, Kind::Commodity, &commodity).await,
        buy_terminal,
        price_mode,
        buy_price_scu,
        buy_price_lot,
        pickup_stop,
        drop_stop,
        destination,
        boxes,
        reward,
        whereabouts,
        stored_at,
        notes: opt(&f.notes),
    })
}

fn load_form_from(form: &FormData, i: usize) -> LoadFormView {
    LoadFormView {
        kind: form.nth("kind", i).into(),
        reward: form.nth("reward", i).into(),
        commodity: form.nth("commodity", i).into(),
        boxes: form.nth("boxes", i).into(),
        // derived from the boxes, never read back from the post
        scu: views::scu_from_boxes(form.nth("boxes", i)),
        price_mode: form.nth("price_mode", i).into(),
        price_scu: form.nth("price_scu", i).into(),
        price_lot: form.nth("price_lot", i).into(),
        balance_before: form.nth("balance_before", i).into(),
        balance_after: form.nth("balance_after", i).into(),
        pickup: form.nth("pickup", i).into(),
        drop: form.nth("drop", i).into(),
        stops: Vec::new(),
        status: form.nth("status", i).into(),
        stored_at: form.nth("stored_at", i).into(),
        notes: form.nth("notes", i).into(),
        show_status: false,
        bought_by: form.nth("bought_by", i).into(),
        users: Vec::new(),
        error: None,
    }
}

/// A crew member picked on a form (`logged_by`, `bought_by`, `sold_by`):
/// blank means the member filling it in; anything else must be a real id.
fn pick_member(raw: &str, fallback: i64, users: &[User], what: &str) -> Result<i64, String> {
    match opt(raw) {
        None => Ok(fallback),
        Some(v) => {
            let id: i64 = v.parse().map_err(|_| format!("{what}: pick a crew member."))?;
            if users.iter().any(|u| u.id == id) {
                Ok(id)
            } else {
                Err(format!("{what}: that crew member doesn't exist."))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GET /  -- the ledger terminal
// ---------------------------------------------------------------------------

pub async fn index(Db(state): Db, user: CurrentUser, session: Session, headers: HeaderMap) -> Response {
    let mode = take_splash_mode(&session, &user, is_landing(&headers)).await;
    render_index(&state, &user, mode, None).await
}

async fn render_index(state: &AppState, user: &CurrentUser, mode: SplashMode, new_run: Option<NewRunFormView>) -> Response {
    let l = match ledger(state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let list = |kind: Kind| async move {
        lookups::names(&state.pool, kind).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, list = kind.label(), "failed to load pick-list");
            Vec::new()
        })
    };
    let default_terminal = repo::latest_sell_terminal(&state.pool).await.ok().flatten().unwrap_or_default();
    let default_ship = repo::latest_ship_name(&state.pool).await.ok().flatten().unwrap_or_default();
    let status = if new_run.as_ref().map_or(false, |f| f.error.is_some()) {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };
    let mut new_run = new_run.unwrap_or_else(|| {
        // start where the crew last sold, with one stop to name; the first
        // load boards at the start and comes off at the last stop
        let route = RouteFormView {
            stops: vec![
                RouteRow { key: "n0".into(), terminal: default_terminal.clone() },
                RouteRow { key: "n1".into(), terminal: String::new() },
            ],
            ..Default::default()
        };
        NewRunFormView {
            bought_at: today().to_string(),
            logged_by: user.id.to_string(),
            ship: default_ship,
            // a run being logged at the terminal hasn't been loaded yet
            status: "awaiting_pickup".into(),
            loads: vec![LoadFormView::blank(route.options())],
            route,
            ..Default::default()
        }
    });
    new_run.users = l.users.clone();
    let options = new_run.route.options();
    for f in &mut new_run.loads {
        f.stops = options.clone();
    }
    html(
        status,
        render_str(&IndexTemplate {
            shell: shell(state, Some(user), "ledger", mode, None).await,
            totals_html: totals_frag(&l, false),
            open_cargo_html: open_cargo_frag(&l, false),
            plans_html: plans_frag(&l, false),
            ledger_html: ledger_frag(&l, false),
            new_run,
            commodities: list(Kind::Commodity).await,
            terminals: list(Kind::Terminal).await,
            ships: list(Kind::Ship).await,
        }),
    )
}

// ---------------------------------------------------------------------------
// POST /voyages  -- new run
// ---------------------------------------------------------------------------

pub async fn create_voyage(Db(state): Db, user: CurrentUser, body: String) -> Response {
    let form = FormData::parse(&body);
    let n = form.count("commodity").max(1);
    let mut view = NewRunFormView {
        bought_at: form.get("bought_at").into(),
        logged_by: form.get("logged_by").into(),
        users: Vec::new(),
        ship: form.get("ship").into(),
        status: form.get("voyage_status").into(),
        stored_at: form.get("voyage_stored_at").into(),
        notes: form.get("voyage_notes").into(),
        route: RouteFormView { stops: route_rows_from(&form), ..Default::default() },
        loads: (0..n).map(|i| load_form_from(&form, i)).collect(),
        error: None,
        open: true,
    };
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    let owner = match pick_member(&view.logged_by, user.id, &users, "Crew member") {
        Ok(id) => id,
        Err(e) => {
            view.error = Some(e);
            return render_index(&state, &user, SplashMode::Hidden, Some(view)).await;
        }
    };
    match parse_new_run(&state, &view).await {
        Ok((voyage, loads)) => {
            if let Err(e) = repo::create_voyage(&state.pool, owner, user.id, &voyage, &loads).await {
                tracing::error!(error = %e, "failed to create voyage");
                view.error = Some("Could not save that run.".into());
                return render_index(&state, &user, SplashMode::Hidden, Some(view)).await;
            }
            Redirect::to("/").into_response()
        }
        Err(e) => {
            view.error = Some(e);
            render_index(&state, &user, SplashMode::Hidden, Some(view)).await
        }
    }
}

async fn parse_new_run(state: &AppState, v: &NewRunFormView) -> Result<(NewVoyage, Vec<NewLoad>), String> {
    let bought_at = parse_date(&v.bought_at, today()).map_err(|e| format!("Bought on: {e}"))?;
    let ship_id = pick_ship(state, &v.ship).await?;
    let whereabouts = parse_whereabouts(&v.status)?.unwrap_or(Whereabouts::Aboard);
    let stored_at = pick_opt(state, Kind::Terminal, &v.stored_at).await;
    if whereabouts == Whereabouts::Stored && stored_at.is_none() {
        return Err("Stored cargo needs a stored-at station.".into());
    }
    if v.loads.is_empty() {
        return Err("A run needs at least one cargo load.".into());
    }
    let route = parse_route(state, &v.route.stops).await?;
    let mut loads = Vec::new();
    for (i, f) in v.loads.iter().enumerate() {
        let p = parse_load_line(state, f, true, 0, &route)
            .await
            .map_err(|e| format!("Load {}: {e}", i + 1))?;
        // the status is for what boards at the start; later pickups wait
        let (whereabouts, stored_at) = if p.pickup_stop > 0 {
            (Whereabouts::AwaitingPickup, None)
        } else {
            (whereabouts, stored_at.clone())
        };
        loads.push(NewLoad {
            kind: p.kind,
            reward: p.reward,
            commodity: p.commodity,
            buy_terminal: p.buy_terminal,
            price_mode: p.price_mode,
            buy_price_scu: p.buy_price_scu,
            buy_price_lot: p.buy_price_lot,
            pickup_stop: p.pickup_stop,
            drop_stop: p.drop_stop,
            destination: p.destination,
            boxes: p.boxes,
            whereabouts,
            stored_at,
            notes: p.notes,
        });
    }
    let route = route.into_iter().map(|(_, t)| t).collect();
    Ok((NewVoyage { ship_id, bought_at, notes: opt(&v.notes), route }, loads))
}

// ---------------------------------------------------------------------------
// voyage header: edit, merge chooser, add load, plain
// ---------------------------------------------------------------------------

/// `GET /voyages/:id/head` -- the header on its own (cancels a chooser/form).
pub async fn voyage_head(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => html(StatusCode::OK, head_frag(VoyageView::new(&v), false)),
        Err(r) => r,
    }
}

fn head_with(v: &Voyage, extra: VoyageExtra, status: StatusCode) -> Response {
    let mut view = VoyageView::new(v);
    view.extra = extra;
    html(status, head_frag(view, false))
}

pub async fn voyage_edit_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => head_with(&v, VoyageExtra::Edit(voyage_form(&state, &v, true).await), StatusCode::OK),
        Err(r) => r,
    }
}

/// `PUT /voyages/:id` -- ship, date, notes, whose run it is and who logged it. Success re-renders Open Cargo
/// (every load's where-now names the hull).
pub async fn voyage_update(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match save_voyage_form(&state, &v, &body).await {
        Err(view) => head_with(&v, VoyageExtra::Edit(view), StatusCode::UNPROCESSABLE_ENTITY),
        Ok(()) => match ledger(&state).await {
            Ok(l) => retarget(open_cargo_response(&l), "#open-cargo", "outerHTML"),
            Err(r) => r,
        },
    }
}

/// The route editor filled in as the voyage stands: stops keyed `s<n>` by
/// their current number, and every open load's pickup and drop.
fn route_form(v: &Voyage) -> RouteFormView {
    let key = |n: u32| format!("s{n}");
    RouteFormView {
        stops: v.route.iter().enumerate().map(|(i, t)| RouteRow { key: key(i as u32), terminal: t.clone() }).collect(),
        at: key(v.at_stop),
        show_at: true,
        loads: v
            .open_loads()
            .map(|l| RouteAssign {
                load_id: l.id,
                label: format!("{} · {} SCU", l.commodity.to_uppercase(), l.scu()),
                pickup: key(l.pickup_stop),
                drop: l.drop_stop.map(key).unwrap_or_default(),
            })
            .collect(),
    }
}

/// The voyage form filled in as the voyage stands, its shares included;
/// `with_route` adds the route editor (open voyages, inline).
async fn voyage_form(state: &AppState, v: &Voyage, with_route: bool) -> VoyageFormView {
    VoyageFormView {
        bought_at: v.bought_at.clone(),
        ship: v.ship.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
        notes: v.notes.clone().unwrap_or_default(),
        entered_by: v.entered_by.to_string(),
        users: repo::list_users(&state.pool).await.unwrap_or_default(),
        shares: share_rows_of(&repo::voyage_shares(&state.pool, v.id).await.unwrap_or_default()),
        owner: v.logged_by.to_string(),
        move_loads: false,
        route: with_route.then(|| route_form(v)),
        error: None,
    }
}

/// The route editor as submitted, checked against the voyage: the new
/// stops, where each old stop went, the open loads' picks, and where the
/// voyage now is. A stop a sold load used can't go.
async fn parse_route_edit(state: &AppState, v: &Voyage, form: &FormData, rf: &RouteFormView) -> Result<RouteEdit, String> {
    let route = parse_route(state, &rf.stops).await?;
    let remap: Vec<(u32, u32)> = (0..v.route.len() as u32)
        .filter_map(|old| stop_index(&route, &format!("s{old}")).map(|n| (old, n)))
        .collect();
    let map = |old: u32| remap.iter().find(|(o, _)| *o == old).map(|(_, n)| *n);
    for l in v.loads.iter().filter(|l| l.is_sold()) {
        let p = map(l.pickup_stop);
        let d = l.drop_stop.map(map);
        let kept = p.is_some() && d.map_or(true, |d| d.is_some_and(|d| Some(d) > p));
        if !kept {
            return Err(format!(
                "{} was already sold on this route, at {} -- keep that stop, and keep it after where it was picked up.",
                l.commodity.to_uppercase(),
                l.drop_stop.map(|d| v.stop_name(d)).unwrap_or_else(|| v.stop_name(l.pickup_stop))
            ));
        }
    }
    let ids = form.all("assign_load");
    let mut assign = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let Ok(id) = id.parse::<i64>() else { continue };
        let Some(l) = v.loads.iter().find(|l| l.id == id && l.is_open()) else { continue };
        let (p, d) = pick_stops(&route, form.nth("assign_pickup", i), form.nth("assign_drop", i))
            .map_err(|e| format!("{}: {e}", l.commodity.to_uppercase()))?;
        assign.push((id, p, d));
    }
    let last = route.len().saturating_sub(1) as u32;
    let at_stop = stop_index(&route, &rf.at).unwrap_or(map(v.at_stop).unwrap_or(0)).min(last);
    Ok(RouteEdit { stops: route.into_iter().map(|(_, t)| t).collect(), remap, assign, at_stop })
}

/// Validates and saves the voyage form (ship, date, notes, owner, who logged it,
/// the voyage-wide shares); on failure hands the form back with its error
/// to re-render.
async fn save_voyage_form(state: &AppState, v: &Voyage, body: &str) -> Result<(), VoyageFormView> {
    let form = FormData::parse(body);
    let mut view = VoyageFormView {
        bought_at: form.get("bought_at").into(),
        ship: form.get("ship").into(),
        notes: form.get("notes").into(),
        entered_by: form.get("entered_by").into(),
        users: repo::list_users(&state.pool).await.unwrap_or_default(),
        shares: share_rows_from(&form),
        owner: form.get("logged_by").into(),
        move_loads: form.get("move_loads") == "1",
        route: (form.count("stop_key") > 0).then(|| RouteFormView {
            stops: route_rows_from(&form),
            at: form.get("at_stop").into(),
            show_at: true,
            loads: form
                .all("assign_load")
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    let load_id = id.parse().unwrap_or(0);
                    RouteAssign {
                        load_id,
                        label: v
                            .loads
                            .iter()
                            .find(|l| l.id == load_id)
                            .map(|l| format!("{} · {} SCU", l.commodity.to_uppercase(), l.scu()))
                            .unwrap_or_default(),
                        pickup: form.nth("assign_pickup", i).into(),
                        drop: form.nth("assign_drop", i).into(),
                    }
                })
                .collect(),
        }),
        error: None,
    };
    let parsed = async {
        let bought_at = parse_date(&view.bought_at, today()).map_err(|e| format!("Bought on: {e}"))?;
        let ship_id = pick_ship(state, &view.ship).await?;
        let logged_by = pick_member(&view.owner, v.logged_by, &view.users, "Crew member")?;
        let entered_by = pick_member(&view.entered_by, v.entered_by, &view.users, "Logged by")?;
        // the run's owner sells by default and keeps the remainder
        let shares = parse_shares(&view.shares, &view.users, logged_by, "run's owner")?;
        let route = match &view.route {
            Some(rf) => Some(parse_route_edit(state, v, &form, rf).await?),
            None => None,
        };
        Ok::<_, String>(EditVoyage { ship_id, bought_at, notes: opt(&view.notes), logged_by, move_loads: view.move_loads, entered_by, shares, route })
    }
    .await;
    let edit = match parsed {
        Ok(edit) => edit,
        Err(e) => {
            view.error = Some(e);
            return Err(view);
        }
    };
    if let Err(e) = repo::update_voyage(&state.pool, v.id, &edit).await {
        tracing::error!(error = %e, "failed to update voyage");
        view.error = Some("Could not save those changes.".into());
        return Err(view);
    }
    Ok(())
}

fn voyage_overlay(v: &Voyage, f: VoyageFormView, status: StatusCode) -> Response {
    let n = v.loads.len();
    let summary = format!(
        "{n} load{} · {}",
        if n == 1 { "" } else { "s" },
        if v.is_fully_sold() { "closed" } else { "still open" }
    );
    html(status, render_str(&VoyageOverlayFrag { id: v.id, summary, f }))
}

/// `GET /voyages/:id/details` -- the voyage form in an overlay, for any
/// voyage: the ledger's way in to one that's already closed.
pub async fn voyage_details_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => voyage_overlay(&v, voyage_form(&state, &v, false).await, StatusCode::OK),
        Err(r) => r,
    }
}

/// `PUT /voyages/:id/details` -- saves the overlay form. Success empties
/// the overlay (which closes it) and refreshes every panel the voyage
/// shows up in.
pub async fn voyage_details_update(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match save_voyage_form(&state, &v, &body).await {
        Err(view) => voyage_overlay(&v, view, StatusCode::UNPROCESSABLE_ENTITY),
        Ok(()) => match ledger(&state).await {
            Ok(l) => html(
                StatusCode::OK,
                format!("{}{}{}", open_cargo_frag(&l, true), oob_common(&l), ledger_frag(&l, true)),
            ),
            Err(r) => r,
        },
    }
}

/// `GET /voyages/:id/merge` -- the "merge into…" chooser under the header.
pub async fn voyage_merge_chooser(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let l = match ledger(&state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let Some(v) = l.voyages.iter().find(|v| v.id == id) else { return not_found("voyage") };
    let choices = l
        .voyages
        .iter()
        .filter(|o| o.id != id && o.open_loads().next().is_some())
        .map(|o| MergeChoice {
            id: o.id,
            label: format!(
                "Voyage {} · {}",
                o.id,
                o.ship.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "—".into())
            ),
        })
        .collect();
    head_with(v, VoyageExtra::Merge(choices), StatusCode::OK)
}

/// `POST /voyages/:id/move` with `voyage_id` -- every open load of the
/// voyage moves there (the header's Merge, and the Suggested card's).
pub async fn voyage_move(Db(state): Db, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let form = FormData::parse(&body);
    let Ok(target) = form.get("voyage_id").trim().parse::<i64>() else {
        return (StatusCode::UNPROCESSABLE_ENTITY, "Pick a voyage to merge into.").into_response();
    };
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    for load in v.open_loads() {
        if let Err(e) = repo::move_load(&state.pool, load.id, MoveTarget::Voyage(target), user.id).await {
            tracing::error!(error = %e, "failed to move load");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not merge").into_response();
        }
    }
    match ledger(&state).await {
        Ok(l) => open_cargo_response(&l),
        Err(r) => r,
    }
}

/// `POST /voyages/:id/advance` -- `[NEXT STOP]`: the voyage reaches the next
/// stop on its route; whatever is picked up there goes aboard.
pub async fn voyage_advance(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    if let Err(r) = find_voyage(&state, id).await {
        return r;
    }
    if let Err(e) = repo::advance_voyage(&state.pool, id).await {
        tracing::error!(error = %e, "failed to advance voyage");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not move the voyage on").into_response();
    }
    match ledger(&state).await {
        Ok(l) => open_cargo_response(&l),
        Err(r) => r,
    }
}

/// `GET /voyages/:id/loads/new` -- single-load form under the header.
pub async fn add_load_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => {
            // picked up where the voyage is now, bound for the last stop
            let mut form = LoadFormView::blank(route_options(&v));
            form.pickup = v.at_stop.to_string();
            if form.drop.parse::<u32>().is_ok_and(|d| d <= v.at_stop) {
                form.drop = String::new();
            }
            form.show_status = true;
            form.bought_by = v.logged_by.to_string();
            form.users = repo::list_users(&state.pool).await.unwrap_or_default();
            head_with(&v, VoyageExtra::AddLoad(form), StatusCode::OK)
        }
        Err(r) => r,
    }
}

/// `POST /voyages/:id/loads`.
pub async fn add_load(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = FormData::parse(&body);
    let mut f = load_form_from(&form, 0);
    f.show_status = true;
    f.stops = route_options(&v);
    f.users = repo::list_users(&state.pool).await.unwrap_or_default();
    let buyer = match pick_member(&f.bought_by, v.logged_by, &f.users, "Bought by") {
        Ok(id) => id,
        Err(e) => {
            f.error = Some(e);
            return head_with(&v, VoyageExtra::AddLoad(f), StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    match parse_load_line(&state, &f, true, 0, &route_of(&v)).await {
        Err(e) => {
            f.error = Some(e);
            head_with(&v, VoyageExtra::AddLoad(f), StatusCode::UNPROCESSABLE_ENTITY)
        }
        Ok(p) => {
            // a pickup the voyage hasn't reached yet waits for it
            let (whereabouts, stored_at) = match p.whereabouts {
                Some(Whereabouts::Stored) => (Whereabouts::Stored, p.stored_at),
                _ if p.pickup_stop > v.at_stop => (Whereabouts::AwaitingPickup, None),
                w => (w.unwrap_or(Whereabouts::Aboard), p.stored_at),
            };
            let new = NewLoad {
                kind: p.kind,
                reward: p.reward,
                commodity: p.commodity,
                buy_terminal: p.buy_terminal,
                price_mode: p.price_mode,
                buy_price_scu: p.buy_price_scu,
                buy_price_lot: p.buy_price_lot,
                pickup_stop: p.pickup_stop,
                drop_stop: p.drop_stop,
                destination: p.destination,
                boxes: p.boxes,
                whereabouts,
                stored_at,
                notes: p.notes,
            };
            if let Err(e) = repo::add_load(&state.pool, id, buyer, &new).await {
                tracing::error!(error = %e, "failed to add load");
                f.error = Some("Could not save that load.".into());
                return head_with(&v, VoyageExtra::AddLoad(f), StatusCode::UNPROCESSABLE_ENTITY);
            }
            match ledger(&state).await {
                Ok(l) => retarget(open_cargo_response(&l), "#open-cargo", "outerHTML"),
                Err(r) => r,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// delete, with confirmation
// ---------------------------------------------------------------------------

fn confirm_for(kind: &'static str, id: i64, v: &Voyage, loads: &[&Load]) -> ConfirmFrag {
    let scu: u32 = loads.iter().map(|l| l.scu()).sum();
    let committed: f64 = loads.iter().map(|l| l.cost() - l.written_off()).sum();
    ConfirmFrag {
        kind,
        id,
        label: match kind {
            "voyage" => format!("Voyage {}", v.id),
            _ => loads.first().map(|l| l.commodity.to_uppercase()).unwrap_or_default(),
        },
        ship_name: v.ship.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "—".into()),
        logged_by_upper: v.entered_by_name.to_uppercase(),
        lines: loads
            .iter()
            .map(|l| ConfirmLine {
                commodity_upper: l.commodity.to_uppercase(),
                scu: l.scu(),
                cost_fmt: crate::templates::fmt_auec(l.cost() - l.written_off()),
            })
            .collect(),
        load_count: loads.len(),
        scu,
        committed_fmt: crate::templates::fmt_auec(committed),
    }
}

/// `GET /voyages/:id/delete` -- the DELETE overlay.
pub async fn voyage_delete_confirm(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => {
            let loads: Vec<&Load> = v.open_loads().collect();
            html(StatusCode::OK, render_str(&confirm_for("voyage", id, &v, &loads)))
        }
        Err(r) => r,
    }
}

/// `DELETE /voyages/:id` -- only from the confirmation's button.
pub async fn voyage_delete(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    if let Err(r) = find_voyage(&state, id).await {
        return r;
    }
    if let Err(e) = repo::delete_voyage(&state.pool, id).await {
        tracing::error!(error = %e, "failed to delete voyage");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not delete voyage").into_response();
    }
    match ledger(&state).await {
        Ok(l) => html(
            StatusCode::OK,
            format!("{}{}{}", open_cargo_frag(&l, false), oob_common(&l), ledger_frag(&l, true)),
        ),
        Err(r) => r,
    }
}

/// `GET /loads/:id/delete`.
pub async fn load_delete_confirm(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    match find_voyage(&state, load.voyage_id).await {
        Ok(v) => html(StatusCode::OK, render_str(&confirm_for("load", id, &v, &[&load]))),
        Err(r) => r,
    }
}

/// `DELETE /loads/:id`.
pub async fn load_delete(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    if let Err(r) = find_load(&state, id).await {
        return r;
    }
    if let Err(e) = repo::delete_load(&state.pool, id).await {
        tracing::error!(error = %e, "failed to delete load");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not delete load").into_response();
    }
    match ledger(&state).await {
        Ok(l) => html(
            StatusCode::OK,
            format!("{}{}{}", open_cargo_frag(&l, false), oob_common(&l), ledger_frag(&l, true)),
        ),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// write off, with confirmation
// ---------------------------------------------------------------------------

/// `GET /loads/:id/write-off` -- the confirmation overlay.
pub async fn load_write_off_confirm(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let live: Vec<_> = load.crates.iter().filter(|c| c.is_live()).collect();
    let scu: u32 = live.iter().map(|c| c.size_scu).sum();
    html(
        StatusCode::OK,
        render_str(&ConfirmWriteOffFrag {
            id,
            voyage_id: load.voyage_id,
            commodity_upper: load.commodity.to_uppercase(),
            by_upper: load.submitted_by_name.to_uppercase(),
            crates: live.len(),
            scu,
            cost_fmt: crate::templates::fmt_auec(scu as f64 * load.buy_price_scu),
        }),
    )
}

/// `POST /loads/:id/write-off` -- every crate still in play is lost; the
/// load leaves Open Cargo without a sale.
pub async fn load_write_off(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    if !load.is_open() {
        return (StatusCode::CONFLICT, "That load is already closed.").into_response();
    }
    if let Err(e) = repo::write_off_load(&state.pool, id).await {
        tracing::error!(error = %e, "failed to write off load");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not write off load").into_response();
    }
    match ledger(&state).await {
        Ok(l) => html(StatusCode::OK, format!("{}{}", open_cargo_frag(&l, false), oob_common(&l))),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// load row: plain, edit, crates, move
// ---------------------------------------------------------------------------

/// A load's tbody (row + whatever is open under it) plus the oob bits a
/// crate-level change touches: totals, plan, and its voyage header.
async fn load_response(state: &AppState, id: i64, extra: LoadExtra, status: StatusCode) -> Response {
    let l = match ledger(state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let Some(mut lv) = load_view(&l, id) else { return not_found("load") };
    lv.extra = extra;
    let head = voyage_view(&l, lv.voyage_id).map(|v| head_frag(v, true)).unwrap_or_default();
    html(status, format!("{}{}{}", load_frag(lv, false), oob_common(&l), head))
}

/// `GET /loads/:id` -- the plain row (closes a manifest or form).
pub async fn load_row(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    load_response(&state, id, LoadExtra::None, StatusCode::OK).await
}

/// `GET /loads/:id/crates` -- the crate manifest under the row.
pub async fn crate_manifest(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    load_response(&state, id, LoadExtra::Manifest, StatusCode::OK).await
}

/// `GET /loads/:id/edit`.
pub async fn load_edit_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let v = match find_voyage(&state, load.voyage_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut form = LoadFormView::from_load(&load, &v);
    form.show_status = true;
    form.users = repo::list_users(&state.pool).await.unwrap_or_default();
    load_response(&state, id, LoadExtra::Edit(form), StatusCode::OK).await
}

/// `PUT /loads/:id/edit` -- incl. status, stop №, stored-at.
pub async fn load_update(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let v = match find_voyage(&state, load.voyage_id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = FormData::parse(&body);
    let mut f = load_form_from(&form, 0);
    f.boxes = views::LoadFormView::from_load(&load, &v).boxes;
    f.stops = route_options(&v);
    f.show_status = true;
    f.users = repo::list_users(&state.pool).await.unwrap_or_default();
    let parsed = match pick_member(&f.bought_by, load.bought_by, &f.users, if load.is_haul() { "Hauled by" } else { "Bought by" }) {
        Ok(buyer) => parse_load_line(&state, &f, false, load.scu(), &route_of(&v)).await.map(|p| (buyer, p)),
        Err(e) => Err(e),
    };
    match parsed {
        Err(e) => {
            f.error = Some(e);
            load_response(&state, id, LoadExtra::Edit(f), StatusCode::UNPROCESSABLE_ENTITY).await
        }
        Ok((buyer, p)) => {
            let edit = EditLoad {
                kind: p.kind,
                reward: p.reward,
                commodity: p.commodity,
                buy_terminal: p.buy_terminal,
                price_mode: p.price_mode,
                buy_price_scu: p.buy_price_scu,
                buy_price_lot: p.buy_price_lot,
                pickup_stop: p.pickup_stop,
                drop_stop: p.drop_stop,
                destination: p.destination,
                whereabouts: p.whereabouts,
                stored_at: p.stored_at,
                notes: p.notes,
                bought_by: buyer,
            };
            if let Err(e) = repo::update_load(&state.pool, id, &edit).await {
                tracing::error!(error = %e, "failed to update load");
                f.error = Some("Could not save those changes.".into());
                return load_response(&state, id, LoadExtra::Edit(f), StatusCode::UNPROCESSABLE_ENTITY).await;
            }
            load_response(&state, id, LoadExtra::None, StatusCode::OK).await
        }
    }
}

/// `POST /loads/:id/move` with `voyage_id` = a voyage id or `new`.
pub async fn load_move(Db(state): Db, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    if let Err(r) = find_load(&state, id).await {
        return r;
    }
    let form = FormData::parse(&body);
    let target = match form.get("voyage_id").trim() {
        "new" => MoveTarget::New,
        other => match other.parse::<i64>() {
            Ok(n) => MoveTarget::Voyage(n),
            Err(_) => return (StatusCode::UNPROCESSABLE_ENTITY, "Pick a voyage, or \"new\".").into_response(),
        },
    };
    if let MoveTarget::New = target {
        // detaching a voyage's only open load would just renumber the voyage
        let l = match ledger(&state).await {
            Ok(l) => l,
            Err(r) => return r,
        };
        if !load_view(&l, id).is_some_and(|lv| lv.can_detach) {
            return (StatusCode::CONFLICT, "That load is already on a voyage of its own.").into_response();
        }
    }
    if let Err(e) = repo::move_load(&state.pool, id, target, user.id).await {
        tracing::error!(error = %e, "failed to move load");
        return (StatusCode::UNPROCESSABLE_ENTITY, "Could not move that load.").into_response();
    }
    match ledger(&state).await {
        Ok(l) => open_cargo_response(&l),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// crates
// ---------------------------------------------------------------------------

/// `POST /crates/:id/condition` -- cycle sound → glitched → lost.
pub async fn crate_condition(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let c = match repo::find_crate(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("crate"),
        Err(e) => {
            tracing::error!(error = %e, "failed to load crate");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not load crate").into_response();
        }
    };
    if let Err(e) = repo::set_crate_condition(&state.pool, id, c.condition.next()).await {
        tracing::error!(error = %e, "failed to set crate condition");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not update crate").into_response();
    }
    load_response(&state, c.load_id, LoadExtra::Manifest, StatusCode::OK).await
}

/// `whereabouts=aboard|stored|awaiting_pickup`, `stored_at=` when stored.
async fn where_from(state: &AppState, form: &FormData) -> Result<(Whereabouts, Option<String>), String> {
    let w = parse_whereabouts(form.get("whereabouts"))?.ok_or("Say where the cargo is.")?;
    let stored_at = pick_opt(state, Kind::Terminal, form.get("stored_at")).await;
    if w == Whereabouts::Stored && stored_at.is_none() {
        return Err("Stored cargo needs a stored-at station.".into());
    }
    Ok((w, stored_at))
}

/// `POST /crates/:id/where` -- toggle aboard ↔ stored (with `stored_at`).
pub async fn crate_where(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let c = match repo::find_crate(&state.pool, id).await {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("crate"),
        Err(e) => {
            tracing::error!(error = %e, "failed to load crate");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not load crate").into_response();
        }
    };
    let form = FormData::parse(&body);
    match where_from(&state, &form).await {
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
        Ok((w, at)) => {
            if let Err(e) = repo::set_crate_where(&state.pool, id, w, at.as_deref()).await {
                tracing::error!(error = %e, "failed to move crate");
                return (StatusCode::INTERNAL_SERVER_ERROR, "could not update crate").into_response();
            }
            load_response(&state, c.load_id, LoadExtra::Manifest, StatusCode::OK).await
        }
    }
}

/// `POST /loads/:id/where` -- `[ALL ABOARD]` / `[ALL STORED]`, and the row's
/// `[LOADED]` (`view=row`).
pub async fn load_where(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    if let Err(r) = find_load(&state, id).await {
        return r;
    }
    let form = FormData::parse(&body);
    match where_from(&state, &form).await {
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
        Ok((w, at)) => {
            if let Err(e) = repo::set_load_where(&state.pool, id, w, at.as_deref()).await {
                tracing::error!(error = %e, "failed to move crates");
                return (StatusCode::INTERNAL_SERVER_ERROR, "could not update crates").into_response();
            }
            // the row's own [LOADED] button answers with the plain row; the
            // manifest's buttons keep the manifest open
            let extra = if form.get("view") == "row" { LoadExtra::None } else { LoadExtra::Manifest };
            load_response(&state, id, extra, StatusCode::OK).await
        }
    }
}

// ---------------------------------------------------------------------------
// close
// ---------------------------------------------------------------------------

/// A close form for `load`, filled in with what it most likely is: today,
/// its destination, the sound crates, and its voyage's shares (less any
/// share held by the load's owner, who sells it and keeps the remainder).
fn blank_close(load: &Load, users: Vec<User>, voyage_shares: &[ShareInput]) -> CloseFormView {
    let shares: Vec<ShareInput> = voyage_shares.iter().filter(|s| s.to_user != load.submitted_by).cloned().collect();
    CloseFormView {
        sold_at: today().to_string(),
        sell_terminal: load.destination.clone().unwrap_or_default(),
        // a haul is paid what the contract promised, as one sum
        price_mode: if load.is_haul() { "lot" } else { "scu" }.into(),
        sell_scu: String::new(),
        sell_lot: load.reward.filter(|_| load.is_haul()).map(|r| views::trim_num(views::to_cents(r))).unwrap_or_default(),
        balance_before: String::new(),
        balance_after: String::new(),
        selected: load.crates.iter().filter(|c| c.default_selected()).map(|c| c.id).collect(),
        write_off_glitched: true,
        shares: share_rows_of(&shares),
        users,
        sold_by: load.submitted_by.to_string(),
        bought_by: load.bought_by.to_string(),
        error: None,
    }
}

/// `GET /loads/:id/close`.
pub async fn close_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    if load.is_sold() {
        return (StatusCode::CONFLICT, "That load is already sold.").into_response();
    }
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    let shares = repo::voyage_shares(&state.pool, load.voyage_id).await.unwrap_or_default();
    load_response(&state, id, LoadExtra::Close(blank_close(&load, users, &shares)), StatusCode::OK).await
}

fn close_form_from(form: &FormData, users: Vec<User>) -> CloseFormView {
    CloseFormView {
        sold_at: form.get("sold_at").into(),
        sell_terminal: form.get("sell_terminal").into(),
        price_mode: form.get("sell_mode").into(),
        sell_scu: form.get("sell_scu").into(),
        sell_lot: form.get("sell_lot").into(),
        balance_before: form.get("sell_bal_before").into(),
        balance_after: form.get("sell_bal_after").into(),
        selected: form.all("crate").iter().filter_map(|v| v.parse().ok()).collect(),
        write_off_glitched: form.get("write_off_glitched") == "1",
        shares: share_rows_from(form),
        users,
        sold_by: form.get("sold_by").into(),
        bought_by: form.get("bought_by").into(),
        error: None,
    }
}

/// The sale fields every sale form shares -- date, terminal, price (per SCU,
/// per lot or from the balance) and shares -- validated for `seller`.
async fn parse_sale(
    state: &AppState,
    f: &CloseFormView,
    users: &[User],
    seller: i64,
) -> Result<(String, String, PriceMode, f64, Vec<ShareInput>), String> {
    let sold_at = parse_date(&f.sold_at, today()).map_err(|e| format!("Sold on: {e}"))?;
    let terminal = opt(&f.sell_terminal).ok_or("Sell terminal is required.")?;
    let sell_terminal = pick(state, Kind::Terminal, &terminal).await;
    let (price_mode, price) = match f.price_mode.as_str() {
        "balance" => (PriceMode::Lot, balance_delta(&f.balance_before, &f.balance_after, true)?),
        mode => match mode.parse().unwrap_or(PriceMode::Scu) {
            PriceMode::Scu => (PriceMode::Scu, parse_money(&f.sell_scu, "Sell price per SCU")?),
            PriceMode::Lot => (PriceMode::Lot, parse_money(&f.sell_lot, "Amount received for the lot")?),
        },
    };
    let shares = parse_shares(&f.shares, users, seller, "seller")?;
    Ok((sold_at, sell_terminal, price_mode, price, shares))
}

/// Share rows as submitted (`share_to`, `share_kind`, … repeated per row).
fn share_rows_from(form: &FormData) -> Vec<ShareFormRow> {
    (0..form.count("share_to"))
        .map(|i| ShareFormRow {
            to_user: form.nth("share_to", i).into(),
            paid_to: form.nth("share_paid_to", i).into(),
            kind: form.nth("share_kind", i).into(),
            amount: form.nth("share_amount", i).into(),
            reason: form.nth("share_reason", i).into(),
        })
        .collect()
}

/// Shares back into form rows, to fill a form in.
fn share_rows_of(shares: &[ShareInput]) -> Vec<ShareFormRow> {
    shares
        .iter()
        .map(|s| ShareFormRow {
            to_user: s.to_user.to_string(),
            paid_to: s.paid_to.map(|p| p.to_string()).unwrap_or_default(),
            kind: s.kind.as_str().into(),
            amount: views::trim_num(s.amount),
            reason: s.reason.clone().unwrap_or_default(),
        })
        .collect()
}

/// Validates share rows for `seller`, who keeps the remainder and so can't
/// hold a share; `who` names them in the error ("seller", "run's owner").
fn parse_shares(rows: &[ShareFormRow], users: &[User], seller: i64, who: &str) -> Result<Vec<ShareInput>, String> {
    let mut shares = Vec::new();
    let mut pct_total = 0.0;
    for (i, s) in rows.iter().enumerate() {
        let to_user: i64 = s.to_user.trim().parse().map_err(|_| format!("Share {}: pick a crew member.", i + 1))?;
        if !users.iter().any(|u| u.id == to_user) {
            return Err(format!("Share {}: that crew member doesn't exist.", i + 1));
        }
        if to_user == seller {
            return Err(format!("Share {}: the {who} can't share with themself -- the remainder is already theirs.", i + 1));
        }
        let paid_to = match opt(&s.paid_to) {
            None => None,
            Some(p) => {
                let p: i64 = p.parse().map_err(|_| format!("Share {}: forfeit-to must be a crew member.", i + 1))?;
                if !users.iter().any(|u| u.id == p) {
                    return Err(format!("Share {}: forfeit-to must be a crew member.", i + 1));
                }
                (p != to_user).then_some(p)
            }
        };
        let kind: ShareKind = s.kind.parse().unwrap_or(ShareKind::Pct);
        let amount = parse_money(&s.amount, &format!("Share {} amount", i + 1))?;
        if kind == ShareKind::Pct {
            if amount > 100.0 {
                return Err(format!("Share {}: a percentage can't exceed 100.", i + 1));
            }
            pct_total += amount;
        }
        shares.push(ShareInput { to_user, paid_to, kind, amount, reason: opt(&s.reason) });
    }
    if pct_total > 100.0 + 1e-9 {
        return Err(format!("Percentage shares add up to {}% -- more than the whole profit.", views::trim_num(pct_total)));
    }
    Ok(shares)
}

/// `POST /loads/:id/close` -- records the sale; answers with the SALE banner.
pub async fn close_load(Db(state): Db, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    if load.is_sold() {
        return (StatusCode::CONFLICT, "That load is already sold.").into_response();
    }
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    let form = FormData::parse(&body);
    let mut f = close_form_from(&form, users.clone());
    let seller = match pick_member(&f.sold_by, load.submitted_by, &users, "Sold by") {
        Ok(id) => id,
        Err(e) => {
            f.error = Some(e);
            return load_response(&state, id, LoadExtra::Close(f), StatusCode::UNPROCESSABLE_ENTITY).await;
        }
    };

    let parsed = parse_sale(&state, &f, &users, seller)
        .await
        .map(|(sold_at, sell_terminal, price_mode, price, shares)| {
            (sold_at, sell_terminal, SaleInput { crate_ids: f.selected.clone(), price_mode, price, shares })
        });

    let (sold_at, sell_terminal, input) = match parsed {
        Ok(x) => x,
        Err(e) => {
            f.error = Some(e);
            return load_response(&state, id, LoadExtra::Close(f), StatusCode::UNPROCESSABLE_ENTITY).await;
        }
    };
    // unsold glitched crates written off count as lost before settling, so a
    // close that leaves nothing else behind closes the load whole
    let mut load = load;
    let written_off = if f.write_off_glitched { load.write_off_glitched(&input.crate_ids) } else { Vec::new() };
    let sale = match settle(&load, &input) {
        Ok(s) => Sale { written_off, ..s },
        Err(e) => {
            f.error = Some(e);
            return load_response(&state, id, LoadExtra::Close(f), StatusCode::UNPROCESSABLE_ENTITY).await;
        }
    };
    let sold_id = match repo::close_load(&state.pool, id, seller, &sale, &sold_at, &sell_terminal).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "failed to close load");
            f.error = Some("Could not record that sale.".into());
            return load_response(&state, id, LoadExtra::Close(f), StatusCode::UNPROCESSABLE_ENTITY).await;
        }
    };
    let l = match ledger(&state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let Some(sold) = l.voyages.iter().flat_map(|v| v.loads.iter()).find(|x| x.id == sold_id) else {
        return not_found("sale");
    };
    let kept_scu: u32 = sale.kept.iter().filter(|c| c.is_live()).map(|c| c.size_scu).sum();
    let seller_name = users.iter().find(|u| u.id == seller).map(|u| u.display_name.clone()).unwrap_or_default();
    let banner = SaleFrag {
        haul: sold.is_haul(),
        who_upper: seller_name.to_uppercase(),
        kept_by_upper: (seller != user.id).then(|| user.display_name.to_uppercase()),
        profit_fmt: views::fmt_signed(sale.profit),
        profit_neg: sale.profit < 0.0,
        commodity_upper: sold.commodity.to_uppercase(),
        scu_sold: sale.scu_sold,
        from_short: views::short_terminal(&sold.buy_terminal),
        to_short: views::short_terminal(&sell_terminal),
        shares: sold.shares.iter().map(ShareView::new).collect(),
        keeps_fmt: crate::templates::fmt_auec(sale.seller_keeps),
        kept_scu,
    };
    let body = format!(
        "{}{}{}{}",
        render_str(&banner),
        oob_common(&l),
        open_cargo_frag(&l, true),
        ledger_frag(&l, true)
    );
    retarget(html(StatusCode::OK, body), "#overlay", "innerHTML")
}

// ---------------------------------------------------------------------------
// editing a recorded sale
// ---------------------------------------------------------------------------

/// A sold load and whether it can be edited as a sale.
async fn find_sale(state: &AppState, id: i64) -> Result<Load, Response> {
    let load = find_load(state, id).await?;
    if !load.was_sold() {
        return Err((StatusCode::CONFLICT, "That load hasn't been sold.").into_response());
    }
    Ok(load)
}

fn sale_overlay(load: &Load, f: CloseFormView, status: StatusCode) -> Response {
    html(
        status,
        render_str(&SaleOverlayFrag {
            haul: load.is_haul(),
            id: load.id,
            voyage_id: load.voyage_id,
            commodity_upper: load.commodity.to_uppercase(),
            scu_sold: load.scu_sold(),
            buy_price_scu: views::trim_num(load.buy_price_scu),
            buy_price_scu_fmt: views::fmt_price(load.buy_price_scu),
            f,
        }),
    )
}

/// `GET /loads/:id/sale` -- the sale editor, filled in as recorded. The
/// price comes up as the lot total: the figure actually received.
pub async fn sale_edit_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_sale(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    let f = CloseFormView {
        sold_at: load.sold_at.clone().unwrap_or_default(),
        sell_terminal: load.sell_terminal.clone().unwrap_or_default(),
        price_mode: "lot".into(),
        sell_scu: load.sell_price_scu.map(|p| views::trim_num(views::to_cents(p))).unwrap_or_default(),
        sell_lot: load.revenue().map(|p| views::trim_num(views::to_cents(p))).unwrap_or_default(),
        balance_before: String::new(),
        balance_after: String::new(),
        selected: Vec::new(),
        write_off_glitched: false,
        shares: load
            .shares
            .iter()
            .map(|s| ShareFormRow {
                to_user: s.to_user.to_string(),
                paid_to: if s.paid_to == s.to_user { String::new() } else { s.paid_to.to_string() },
                kind: s.kind.as_str().into(),
                amount: views::trim_num(s.amount),
                reason: s.reason.clone().unwrap_or_default(),
            })
            .collect(),
        users,
        sold_by: load.submitted_by.to_string(),
        bought_by: load.bought_by.to_string(),
        error: None,
    };
    sale_overlay(&load, f, StatusCode::OK)
}

/// `PUT /loads/:id/sale` -- saves the correction: shares are paid again on
/// the new profit. Success empties the overlay (closing it) and refreshes
/// the ledger and totals.
pub async fn sale_update(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let load = match find_sale(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    let mut f = close_form_from(&FormData::parse(&body), users.clone());
    let parsed = async {
        let seller = pick_member(&f.sold_by, load.submitted_by, &users, "Sold by")?;
        let buyer = pick_member(&f.bought_by, load.bought_by, &users, if load.is_haul() { "Hauled by" } else { "Bought by" })?;
        let (sold_at, sell_terminal, price_mode, price, shares) = parse_sale(&state, &f, &users, seller).await?;
        let resale = resettle(&load, price_mode, price, &shares)?;
        Ok::<_, String>((seller, buyer, sold_at, sell_terminal, resale))
    }
    .await;
    let (seller, buyer, sold_at, sell_terminal, resale) = match parsed {
        Ok(x) => x,
        Err(e) => {
            f.error = Some(e);
            return sale_overlay(&load, f, StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    if let Err(e) = repo::update_sale(&state.pool, id, seller, buyer, &sold_at, &sell_terminal, &resale).await {
        tracing::error!(error = %e, "failed to update sale");
        f.error = Some("Could not save those changes.".into());
        return sale_overlay(&load, f, StatusCode::UNPROCESSABLE_ENTITY);
    }
    match ledger(&state).await {
        Ok(l) => html(StatusCode::OK, format!("{}{}", oob_common(&l), ledger_frag(&l, true))),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// leaderboard and service record
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BoardQuery {
    #[serde(default)]
    pub sort: String,
}

/// `GET /board?sort=profit|runs|scu`.
pub async fn board(Db(state): Db, user: CurrentUser, Query(q): Query<BoardQuery>) -> Response {
    let l = match ledger(&state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    html(
        StatusCode::OK,
        render_str(&BoardTemplate {
            shell: shell(&state, Some(&user), "board", SplashMode::Hidden, None).await,
            board: board_view(&l.users, &l.voyages, SortKey::parse(&q.sort)),
        }),
    )
}

/// `GET /crew/:id` -- verdict banner + service record fragment.
pub async fn crew(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let l = match ledger(&state).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let Some(u) = l.users.iter().find(|u| u.id == id) else { return not_found("crew member") };
    let rec = service_record(u, &l.users, &l.voyages);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    html(StatusCode::OK, render_str(&VerdictFrag { r: record_view(&rec, seed) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_data_keeps_repeated_fields_in_order() {
        let f = FormData::parse("commodity=Laranite&boxes=2%C3%9732&commodity=Agricium&boxes=4x8&ship=Hull-C");
        assert_eq!(f.count("commodity"), 2);
        assert_eq!(f.nth("commodity", 1), "Agricium");
        assert_eq!(f.nth("boxes", 0), "2×32");
        assert_eq!(f.get("ship"), "Hull-C");
        assert_eq!(f.get("missing"), "");
        assert_eq!(f.nth("boxes", 5), "");
    }

    #[test]
    fn a_posted_scu_is_ignored_for_the_boxes_total() {
        let f = FormData::parse("boxes=2x32%2C+4x8&scu=999&boxes=3x8&boxes=bogus");
        assert_eq!(load_form_from(&f, 0).scu, "96");
        assert_eq!(load_form_from(&f, 1).scu, "24");
        assert_eq!(load_form_from(&f, 2).scu, "");
    }

    #[test]
    fn whereabouts_and_money_parse_forgivingly() {
        assert_eq!(parse_whereabouts("in_transit").unwrap(), Some(Whereabouts::Aboard));
        assert_eq!(parse_whereabouts("").unwrap(), None);
        assert!(parse_whereabouts("lost").is_err());
        assert_eq!(parse_money("1,250.5", "x").unwrap(), 1250.5);
        assert!(parse_money("-1", "x").is_err());
        assert!(parse_money("", "x").is_err());
    }

    #[test]
    fn balance_delta_is_the_price_and_refuses_the_wrong_direction() {
        assert_eq!(balance_delta("1,500,000", "1,262,400", false).unwrap(), 237_600.0);
        assert_eq!(balance_delta("1262400", "1650000", true).unwrap(), 387_600.0);
        assert_eq!(balance_delta("500", "500", false).unwrap(), 0.0);
        assert!(balance_delta("1000", "1200", false).unwrap_err().contains("swapped"));
        assert!(balance_delta("1200", "1000", true).unwrap_err().contains("swapped"));
        assert!(balance_delta("", "1000", false).unwrap_err().contains("Balance before"));
        assert!(balance_delta("1000", "", true).unwrap_err().contains("Balance after"));
    }

    fn crew() -> Vec<User> {
        [(1, "Lord Gig"), (2, "JT"), (3, "Crew-03")]
            .iter()
            .map(|(id, n)| User { id: *id, handle: n.to_lowercase(), display_name: n.to_string(), created_at: String::new(), avatar_url: None })
            .collect()
    }

    fn row(to: &str, kind: &str, amount: &str) -> ShareFormRow {
        ShareFormRow { to_user: to.into(), paid_to: String::new(), kind: kind.into(), amount: amount.into(), reason: String::new() }
    }

    #[test]
    fn a_close_starts_from_the_voyage_split_less_the_sellers_own_share() {
        use model::fixtures::{load, sound};
        let mut l = load(1, 10.0, vec![sound(1, 8)]);
        l.submitted_by = 2; // JT's load, on a voyage where JT holds a share
        let voyage = parse_shares(&[row("2", "pct", "33.33"), row("3", "pct", "33.33")], &crew(), 1, "run's owner").unwrap();
        let f = blank_close(&l, crew(), &voyage);
        assert_eq!(f.shares.len(), 1);
        assert_eq!((f.shares[0].to_user.as_str(), f.shares[0].amount.as_str()), ("3", "33.33"));
    }

    #[test]
    fn voyage_shares_refuse_the_owner_and_more_than_the_whole() {
        let err = parse_shares(&[row("1", "pct", "10")], &crew(), 1, "run's owner").unwrap_err();
        assert!(err.contains("run's owner can't share"), "{err}");
        let err = parse_shares(&[row("2", "pct", "60"), row("3", "pct", "50")], &crew(), 1, "run's owner").unwrap_err();
        assert!(err.contains("110%"), "{err}");
        // flat fees don't count toward the 100 %
        assert!(parse_shares(&[row("2", "pct", "100"), row("3", "flat", "5000")], &crew(), 1, "run's owner").is_ok());
    }
}
