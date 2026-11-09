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

use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::{take_splash_mode, CurrentUser, SplashMode};
use crate::lookups::{self, Kind};
use crate::templates::{
    render_str, BoardTemplate, ConfirmFrag, ConfirmLine, IndexTemplate, LedgerFrag, LoadFrag,
    OpenCargoFrag, PlansFrag, SaleFrag, Shell, TotalsFrag, VerdictFrag, VoyageHeadFrag,
};
use crate::AppState;
use model::{
    parse_boxes, parse_date, service_record, settle, today, Load, PriceMode, SaleInput, ShareInput,
    ShareKind, SortKey, Totals, User, Voyage, Whereabouts,
};
use repo::{EditLoad, EditVoyage, MoveTarget, NewLoad, NewVoyage};
use views::{
    board_view, ledger_rows, plan_cards, record_view, CloseFormView, LoadExtra, LoadFormView,
    LoadView, MergeChoice, NewRunFormView, ShareFormRow, ShareView, TotalsView, VoyageExtra,
    VoyageFormView, VoyageView,
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
            .map(|x| LoadView::new(x, v.hull_label().as_deref()))
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
/// (a hull needs an sc-cargo.space slug, and the 80-ship list is the site's).
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

/// Everything a load line needs, validated and canonicalised. Shared by New
/// Run, Add Load and Edit.
struct LoadParts {
    commodity: String,
    buy_terminal: String,
    price_mode: PriceMode,
    buy_price_scu: f64,
    buy_price_lot: f64,
    stop_no: u32,
    destination: Option<String>,
    boxes: model::BoxMix,
    whereabouts: Option<Whereabouts>,
    stored_at: Option<String>,
    notes: Option<String>,
}

async fn parse_load_line(state: &AppState, f: &LoadFormView, boxes_editable: bool, existing_scu: u32) -> Result<LoadParts, String> {
    let commodity = opt(&f.commodity).ok_or("Commodity is required.")?;
    let buy_terminal = opt(&f.terminal).ok_or("Buy terminal is required.")?;
    let (boxes, scu) = if boxes_editable {
        let boxes = parse_boxes(&f.boxes).map_err(|e| format!("Boxes: {e}"))?;
        let total: u32 = boxes.iter().map(|(n, s)| n * s).sum();
        if let Some(typed) = opt(&f.scu) {
            let typed: f64 = typed.parse().map_err(|_| format!("SCU: \"{typed}\" isn't a number."))?;
            if (typed - total as f64).abs() > 1e-9 {
                return Err(format!(
                    "SCU says {} but the boxes add up to {total} -- fix one or the other (crates define the SCU)."
                , views::trim_num(typed)));
            }
        }
        (boxes, total)
    } else {
        (Vec::new(), existing_scu)
    };
    let price_mode: PriceMode = f.price_mode.parse().unwrap_or(PriceMode::Scu);
    let price = match price_mode {
        PriceMode::Scu => parse_money(&f.price_scu, "Buy price per SCU")?,
        PriceMode::Lot => parse_money(&f.price_lot, "Price paid for the lot")?,
    };
    let (buy_price_scu, buy_price_lot) = model::derive_prices(price_mode, price, scu);
    let stop_no: u32 = match opt(&f.stop_no) {
        None => 1,
        Some(s) => s.parse().ok().filter(|n| *n >= 1).ok_or("Stop № must be 1 or more.")?,
    };
    let whereabouts = parse_whereabouts(&f.status)?;
    let stored_at = pick_opt(state, Kind::Terminal, &f.stored_at).await;
    if whereabouts == Some(Whereabouts::Stored) && stored_at.is_none() {
        return Err("Stored cargo needs a stored-at station.".into());
    }
    Ok(LoadParts {
        commodity: pick(state, Kind::Commodity, &commodity).await,
        buy_terminal: pick(state, Kind::Terminal, &buy_terminal).await,
        price_mode,
        buy_price_scu,
        buy_price_lot,
        stop_no,
        destination: pick_opt(state, Kind::Terminal, &f.destination).await,
        boxes,
        whereabouts,
        stored_at,
        notes: opt(&f.notes),
    })
}

fn load_form_from(form: &FormData, i: usize) -> LoadFormView {
    LoadFormView {
        commodity: form.nth("commodity", i).into(),
        boxes: form.nth("boxes", i).into(),
        scu: form.nth("scu", i).into(),
        price_mode: form.nth("price_mode", i).into(),
        price_scu: form.nth("price_scu", i).into(),
        price_lot: form.nth("price_lot", i).into(),
        terminal: form.nth("terminal", i).into(),
        stop_no: form.nth("stop_no", i).into(),
        destination: form.nth("destination", i).into(),
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

pub async fn index(State(state): State<AppState>, user: CurrentUser, session: Session) -> Response {
    let mode = take_splash_mode(&session, &user).await;
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
    let mut new_run = new_run.unwrap_or_else(|| NewRunFormView {
        bought_at: today().to_string(),
        logged_by: user.id.to_string(),
        ship: default_ship,
        status: "aboard".into(),
        loads: vec![LoadFormView::blank(&default_terminal)],
        ..Default::default()
    });
    new_run.users = l.users.clone();
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

pub async fn create_voyage(State(state): State<AppState>, user: CurrentUser, body: String) -> Response {
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
            if let Err(e) = repo::create_voyage(&state.pool, owner, &voyage, &loads).await {
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
    let mut loads = Vec::new();
    for (i, f) in v.loads.iter().enumerate() {
        let p = parse_load_line(state, f, true, 0)
            .await
            .map_err(|e| format!("Load {}: {e}", i + 1))?;
        loads.push(NewLoad {
            commodity: p.commodity,
            buy_terminal: p.buy_terminal,
            price_mode: p.price_mode,
            buy_price_scu: p.buy_price_scu,
            buy_price_lot: p.buy_price_lot,
            stop_no: p.stop_no,
            destination: p.destination,
            boxes: p.boxes,
            whereabouts,
            stored_at: stored_at.clone(),
            notes: p.notes,
        });
    }
    Ok((NewVoyage { ship_id, bought_at, notes: opt(&v.notes) }, loads))
}

// ---------------------------------------------------------------------------
// voyage header: edit, merge chooser, add load, plain
// ---------------------------------------------------------------------------

/// `GET /voyages/:id/head` -- the header on its own (cancels a chooser/form).
pub async fn voyage_head(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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

pub async fn voyage_edit_form(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => {
            let form = VoyageFormView {
                bought_at: v.bought_at.clone(),
                ship: v.ship.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                notes: v.notes.clone().unwrap_or_default(),
                error: None,
            };
            head_with(&v, VoyageExtra::Edit(form), StatusCode::OK)
        }
        Err(r) => r,
    }
}

/// `PUT /voyages/:id` -- ship, date, notes. Success re-renders Open Cargo
/// (every load's where-now names the hull).
pub async fn voyage_update(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = FormData::parse(&body);
    let mut view = VoyageFormView {
        bought_at: form.get("bought_at").into(),
        ship: form.get("ship").into(),
        notes: form.get("notes").into(),
        error: None,
    };
    let parsed = async {
        let bought_at = parse_date(&view.bought_at, today()).map_err(|e| format!("Bought on: {e}"))?;
        let ship_id = pick_ship(&state, &view.ship).await?;
        Ok::<_, String>(EditVoyage { ship_id, bought_at, notes: opt(&view.notes) })
    }
    .await;
    match parsed {
        Err(e) => {
            view.error = Some(e);
            head_with(&v, VoyageExtra::Edit(view), StatusCode::UNPROCESSABLE_ENTITY)
        }
        Ok(edit) => {
            if let Err(e) = repo::update_voyage(&state.pool, id, &edit).await {
                tracing::error!(error = %e, "failed to update voyage");
                view.error = Some("Could not save those changes.".into());
                return head_with(&v, VoyageExtra::Edit(view), StatusCode::UNPROCESSABLE_ENTITY);
            }
            match ledger(&state).await {
                Ok(l) => retarget(open_cargo_response(&l), "#open-cargo", "outerHTML"),
                Err(r) => r,
            }
        }
    }
}

/// `GET /voyages/:id/merge` -- the "merge into…" chooser under the header.
pub async fn voyage_merge_chooser(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
pub async fn voyage_move(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
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

/// `GET /voyages/:id/loads/new` -- single-load form under the header.
pub async fn add_load_form(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => {
            let terminal = v
                .loads
                .last()
                .map(|l| l.buy_terminal.clone())
                .unwrap_or_default();
            let mut form = LoadFormView::blank(&terminal);
            form.show_status = true;
            form.bought_by = v.logged_by.to_string();
            form.users = repo::list_users(&state.pool).await.unwrap_or_default();
            head_with(&v, VoyageExtra::AddLoad(form), StatusCode::OK)
        }
        Err(r) => r,
    }
}

/// `POST /voyages/:id/loads`.
pub async fn add_load(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let v = match find_voyage(&state, id).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = FormData::parse(&body);
    let mut f = load_form_from(&form, 0);
    f.show_status = true;
    f.users = repo::list_users(&state.pool).await.unwrap_or_default();
    let buyer = match pick_member(&f.bought_by, v.logged_by, &f.users, "Bought by") {
        Ok(id) => id,
        Err(e) => {
            f.error = Some(e);
            return head_with(&v, VoyageExtra::AddLoad(f), StatusCode::UNPROCESSABLE_ENTITY);
        }
    };
    match parse_load_line(&state, &f, true, 0).await {
        Err(e) => {
            f.error = Some(e);
            head_with(&v, VoyageExtra::AddLoad(f), StatusCode::UNPROCESSABLE_ENTITY)
        }
        Ok(p) => {
            let new = NewLoad {
                commodity: p.commodity,
                buy_terminal: p.buy_terminal,
                price_mode: p.price_mode,
                buy_price_scu: p.buy_price_scu,
                buy_price_lot: p.buy_price_lot,
                stop_no: p.stop_no,
                destination: p.destination,
                boxes: p.boxes,
                whereabouts: p.whereabouts.unwrap_or(Whereabouts::Aboard),
                stored_at: p.stored_at,
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
        logged_by_upper: v.logged_by_name.to_uppercase(),
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
pub async fn voyage_delete_confirm(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match find_voyage(&state, id).await {
        Ok(v) => {
            let loads: Vec<&Load> = v.open_loads().collect();
            html(StatusCode::OK, render_str(&confirm_for("voyage", id, &v, &loads)))
        }
        Err(r) => r,
    }
}

/// `DELETE /voyages/:id` -- only from the confirmation's button.
pub async fn voyage_delete(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
pub async fn load_delete_confirm(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
pub async fn load_delete(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
pub async fn load_row(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    load_response(&state, id, LoadExtra::None, StatusCode::OK).await
}

/// `GET /loads/:id/crates` -- the crate manifest under the row.
pub async fn crate_manifest(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    load_response(&state, id, LoadExtra::Manifest, StatusCode::OK).await
}

/// `GET /loads/:id/edit`.
pub async fn load_edit_form(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let mut form = LoadFormView::from_load(&load);
    form.show_status = true;
    load_response(&state, id, LoadExtra::Edit(form), StatusCode::OK).await
}

/// `PUT /loads/:id/edit` -- incl. status, stop №, stored-at.
pub async fn load_update(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    let form = FormData::parse(&body);
    let mut f = load_form_from(&form, 0);
    f.boxes = views::LoadFormView::from_load(&load).boxes;
    f.show_status = true;
    match parse_load_line(&state, &f, false, load.scu()).await {
        Err(e) => {
            f.error = Some(e);
            load_response(&state, id, LoadExtra::Edit(f), StatusCode::UNPROCESSABLE_ENTITY).await
        }
        Ok(p) => {
            let edit = EditLoad {
                commodity: p.commodity,
                buy_terminal: p.buy_terminal,
                price_mode: p.price_mode,
                buy_price_scu: p.buy_price_scu,
                buy_price_lot: p.buy_price_lot,
                stop_no: p.stop_no,
                destination: p.destination,
                whereabouts: p.whereabouts,
                stored_at: p.stored_at,
                notes: p.notes,
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
pub async fn load_move(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
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
pub async fn crate_condition(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
pub async fn crate_where(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
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

/// `POST /loads/:id/where` -- `[ALL ABOARD]` / `[ALL STORED]`.
pub async fn load_where(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
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
            load_response(&state, id, LoadExtra::Manifest, StatusCode::OK).await
        }
    }
}

// ---------------------------------------------------------------------------
// close
// ---------------------------------------------------------------------------

fn blank_close(load: &Load, users: Vec<User>) -> CloseFormView {
    CloseFormView {
        sold_at: today().to_string(),
        sell_terminal: load.destination.clone().unwrap_or_default(),
        price_mode: "scu".into(),
        sell_scu: String::new(),
        sell_lot: String::new(),
        selected: load.crates.iter().filter(|c| c.default_selected()).map(|c| c.id).collect(),
        shares: Vec::new(),
        users,
        sold_by: load.submitted_by.to_string(),
        error: None,
    }
}

/// `GET /loads/:id/close`.
pub async fn close_form(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    let load = match find_load(&state, id).await {
        Ok(l) => l,
        Err(r) => return r,
    };
    if load.is_sold() {
        return (StatusCode::CONFLICT, "That load is already sold.").into_response();
    }
    let users = repo::list_users(&state.pool).await.unwrap_or_default();
    load_response(&state, id, LoadExtra::Close(blank_close(&load, users)), StatusCode::OK).await
}

fn close_form_from(form: &FormData, users: Vec<User>) -> CloseFormView {
    let n = form.count("share_to");
    CloseFormView {
        sold_at: form.get("sold_at").into(),
        sell_terminal: form.get("sell_terminal").into(),
        price_mode: form.get("sell_mode").into(),
        sell_scu: form.get("sell_scu").into(),
        sell_lot: form.get("sell_lot").into(),
        selected: form.all("crate").iter().filter_map(|v| v.parse().ok()).collect(),
        shares: (0..n)
            .map(|i| ShareFormRow {
                to_user: form.nth("share_to", i).into(),
                paid_to: form.nth("share_paid_to", i).into(),
                kind: form.nth("share_kind", i).into(),
                amount: form.nth("share_amount", i).into(),
                reason: form.nth("share_reason", i).into(),
            })
            .collect(),
        users,
        sold_by: form.get("sold_by").into(),
        error: None,
    }
}

/// `POST /loads/:id/close` -- records the sale; answers with the SALE banner.
pub async fn close_load(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, body: String) -> Response {
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

    let parsed: Result<(String, String, SaleInput), String> = async {
        let sold_at = parse_date(&f.sold_at, today()).map_err(|e| format!("Sold on: {e}"))?;
        let terminal = opt(&f.sell_terminal).ok_or("Sell terminal is required.")?;
        let sell_terminal = pick(&state, Kind::Terminal, &terminal).await;
        let price_mode: PriceMode = f.price_mode.parse().unwrap_or(PriceMode::Scu);
        let price = match price_mode {
            PriceMode::Scu => parse_money(&f.sell_scu, "Sell price per SCU")?,
            PriceMode::Lot => parse_money(&f.sell_lot, "Amount received for the lot")?,
        };
        let mut shares = Vec::new();
        for (i, s) in f.shares.iter().enumerate() {
            let to_user: i64 = s.to_user.trim().parse().map_err(|_| format!("Share {}: pick a crew member.", i + 1))?;
            if !users.iter().any(|u| u.id == to_user) {
                return Err(format!("Share {}: that crew member doesn't exist.", i + 1));
            }
            if to_user == seller {
                return Err(format!("Share {}: the seller can't share with themself -- the remainder is already theirs.", i + 1));
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
            if kind == ShareKind::Pct && amount > 100.0 {
                return Err(format!("Share {}: a percentage can't exceed 100.", i + 1));
            }
            shares.push(ShareInput { to_user, paid_to, kind, amount, reason: opt(&s.reason) });
        }
        Ok((sold_at, sell_terminal, SaleInput { crate_ids: f.selected.clone(), price_mode, price, shares }))
    }
    .await;

    let (sold_at, sell_terminal, input) = match parsed {
        Ok(x) => x,
        Err(e) => {
            f.error = Some(e);
            return load_response(&state, id, LoadExtra::Close(f), StatusCode::UNPROCESSABLE_ENTITY).await;
        }
    };
    let sale = match settle(&load, &input) {
        Ok(s) => s,
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
// leaderboard and service record
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BoardQuery {
    #[serde(default)]
    pub sort: String,
}

/// `GET /board?sort=profit|runs|scu`.
pub async fn board(State(state): State<AppState>, user: CurrentUser, Query(q): Query<BoardQuery>) -> Response {
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
pub async fn crew(State(state): State<AppState>, _user: CurrentUser, Path(id): Path<i64>) -> Response {
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
    fn whereabouts_and_money_parse_forgivingly() {
        assert_eq!(parse_whereabouts("in_transit").unwrap(), Some(Whereabouts::Aboard));
        assert_eq!(parse_whereabouts("").unwrap(), None);
        assert!(parse_whereabouts("lost").is_err());
        assert_eq!(parse_money("1,250.5", "x").unwrap(), 1250.5);
        assert!(parse_money("-1", "x").is_err());
        assert!(parse_money("", "x").is_err());
    }
}
