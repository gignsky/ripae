//! The fleet tracker (`/fleet`): the crew's actual hulls -- ships and
//! ground vehicles -- where each one is, what it carries, what it costs to
//! claim, and which voyage it's flying. `model` holds the rules, `repo` the
//! SQL, `views` the formatted shapes; this file the handlers.
//!
//! Same htmx shape as the ledger: quick actions swap `#fleet-body`; the
//! add / edit form opens in `#overlay` and answers a success with an empty
//! overlay (which closes it) plus `#fleet-body` out of band. Mutations
//! re-render with the page's own filters, read from `HX-Current-URL`.

pub mod model;
pub mod repo;
pub mod views;

use askama::Template;
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::auth::{CurrentUser, SplashMode};
use crate::lookups::{self, Kind};
use crate::sandbox::Db;
use crate::stowage::model::User;
use crate::stowage::{self, FormData};
use crate::templates::{render_str, Shell};
use crate::AppState;
use model::{check_carry, Action, HullKind, HullStatus, Node};
use repo::{Hull, HullInput};
use views::{fleet_view, FleetQuery, FleetView, HullFormView};

#[derive(Template)]
#[template(path = "fleet.html")]
pub struct FleetTemplate {
    pub shell: Shell,
    pub fleet_html: String,
    pub fv_q: FleetQuery,
    pub owners: Vec<User>,
    pub models: Vec<String>,
    pub locations: Vec<String>,
}

#[derive(Template)]
#[template(path = "partials/fleet_body.html")]
pub struct FleetBodyFrag {
    pub fv: FleetView,
    pub oob: bool,
}

#[derive(Template)]
#[template(path = "partials/hull_form.html")]
pub struct HullFormFrag {
    pub f: HullFormView,
}

fn html(status: StatusCode, body: String) -> Response {
    (status, axum::response::Html(body)).into_response()
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn fail(e: anyhow::Error, what: &str) -> Response {
    tracing::error!(error = %e, "fleet: {what}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("could not {what}")).into_response()
}

/// The page's filters on a request htmx made from it.
fn query_of(headers: &HeaderMap) -> FleetQuery {
    let url = headers.get("hx-current-url").and_then(|v| v.to_str().ok()).unwrap_or("");
    FleetQuery::from_query(url.split_once('?').map(|(_, q)| q).unwrap_or(""))
}

async fn body(state: &AppState, q: FleetQuery, oob: bool) -> Result<String, Response> {
    let hulls = repo::list_hulls(&state.pool).await.map_err(|e| fail(e, "list the fleet"))?;
    let voyages = stowage::repo::list_voyages(&state.pool).await.map_err(|e| fail(e, "list voyages"))?;
    Ok(render_str(&FleetBodyFrag { fv: fleet_view(&hulls, &voyages, q), oob }))
}

async fn body_response(state: &AppState, headers: &HeaderMap) -> Response {
    match body(state, query_of(headers), false).await {
        Ok(b) => html(StatusCode::OK, b),
        Err(r) => r,
    }
}

/// `GET /fleet?owner=&status=&sort=`.
pub async fn page(Db(state): Db, user: CurrentUser, RawQuery(q): RawQuery) -> Response {
    let q = FleetQuery::from_query(q.as_deref().unwrap_or(""));
    let fleet_html = match body(&state, q.clone(), false).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let mut models = lookups::names(&state.pool, Kind::Ship).await.unwrap_or_default();
    models.extend(repo::off_list_models(&state.pool).await.unwrap_or_default());
    let mut locations = lookups::names(&state.pool, Kind::Terminal).await.unwrap_or_default();
    for l in repo::location_names(&state.pool).await.unwrap_or_default() {
        if !locations.iter().any(|t| t.eq_ignore_ascii_case(&l)) {
            locations.push(l);
        }
    }
    locations.sort_by_key(|l| l.to_lowercase());
    html(
        StatusCode::OK,
        render_str(&FleetTemplate {
            shell: stowage::shell(&state, Some(&user), "fleet", SplashMode::Hidden, None).await,
            fleet_html,
            fv_q: q,
            owners: stowage::repo::list_users(&state.pool).await.unwrap_or_default(),
            models,
            locations,
        }),
    )
}

// ---------------------------------------------------------------------------
// add / edit / remove
// ---------------------------------------------------------------------------

async fn form_response(state: &AppState, mut f: HullFormView, status: StatusCode) -> Response {
    f.users = stowage::repo::list_users(&state.pool).await.unwrap_or_default();
    f.options_from(&repo::list_hulls(&state.pool).await.unwrap_or_default());
    html(status, render_str(&HullFormFrag { f }))
}

/// `GET /hulls/new` -- the add form, in the overlay. Owner defaults to you.
pub async fn new_form(Db(state): Db, user: CurrentUser) -> Response {
    let f = HullFormView { owner: user.id.to_string(), kind: "ship".into(), status: "stored".into(), ..Default::default() };
    form_response(&state, f, StatusCode::OK).await
}

pub async fn edit_form(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>) -> Response {
    match repo::find_hull(&state.pool, id).await {
        Ok(Some(h)) => form_response(&state, HullFormView::of(&h), StatusCode::OK).await,
        Ok(None) => (StatusCode::NOT_FOUND, "No such hull.").into_response(),
        Err(e) => fail(e, "load that hull"),
    }
}

fn form_from(form: &FormData, id: Option<i64>) -> HullFormView {
    HullFormView {
        id,
        owner: form.get("owner").into(),
        model: form.get("model").into(),
        nickname: form.get("nickname").into(),
        kind: form.get("kind").into(),
        status: form.get("status").into(),
        location: form.get("location").into(),
        carried_in: form.get("carried_in").into(),
        loaner: form.get("loaner") == "1",
        loaner_for: form.get("loaner_for").into(),
        insurance_claim: form.get("insurance_claim").into(),
        insurance_type: form.get("insurance_type").into(),
        notes: form.get("notes").into(),
        ..Default::default()
    }
}

fn parse_id(raw: &str, what: &str) -> Result<Option<i64>, String> {
    match opt(raw) {
        None => Ok(None),
        Some(v) => v.parse().map(Some).map_err(|_| format!("{what}: pick one from the list.")),
    }
}

/// Validates the add / edit form against the crew and the fleet as it stands.
async fn parse_form(state: &AppState, f: &HullFormView, hulls: &[Hull]) -> Result<HullInput, String> {
    let users = stowage::repo::list_users(&state.pool).await.unwrap_or_default();
    let owner_id = parse_id(&f.owner, "Owner")?.ok_or("Owner: whose hull is it?")?;
    if !users.iter().any(|u| u.id == owner_id) {
        return Err("Owner: that crew member doesn't exist.".into());
    }
    let typed = opt(&f.model).ok_or("Model: name the ship or vehicle (pick from the list, or type it).")?;
    // on the list: its spelling and its capacity; off it: kept as typed
    let (ship_id, model) = match stowage::repo::find_ship_by_name(&state.pool, &typed).await {
        Ok(Some(s)) => (Some(s.id), s.name),
        Ok(None) => (None, typed),
        Err(e) => {
            tracing::error!(error = %e, "failed to look up ship");
            return Err("Could not check the ship list.".into());
        }
    };
    let kind = HullKind::parse(&f.kind).ok_or("Kind: ship or ground vehicle.")?;
    let status = HullStatus::parse(&f.status).ok_or("Status: pick one.")?;
    let location = match opt(&f.location) {
        Some(l) => Some(lookups::canonical(&state.pool, Kind::Terminal, &l).await.unwrap_or(l)),
        None => None,
    };
    let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
    let me = f.id.unwrap_or(-1);
    let carried_in = parse_id(&f.carried_in, "Carried in")?;
    if let Some(c) = carried_in {
        check_carry(&nodes, me, status, c).map_err(|e| format!("Carried in: {e}"))?;
    }
    let loaner_for = parse_id(&f.loaner_for, "Loaner for")?;
    if let Some(p) = loaner_for {
        let Some(pledge) = hulls.iter().find(|h| h.id == p) else { return Err("Loaner for: that hull doesn't exist.".into()) };
        if p == me {
            return Err("Loaner for: a hull can't be its own loaner.".into());
        }
        if pledge.owner_id != owner_id {
            return Err(format!("Loaner for: {} belongs to {} -- a loaner is lent to the pledge's owner.", pledge.label(), pledge.owner_name));
        }
        if pledge.loaner {
            return Err("Loaner for: that hull is a loaner itself.".into());
        }
    }
    let insurance_claim = match opt(&f.insurance_claim) {
        None => None,
        Some(v) => {
            let n: f64 = v.replace(',', "").parse().map_err(|_| "Insurance claim: a number of aUEC.".to_string())?;
            if !(n >= 0.0) {
                return Err("Insurance claim: can't be negative.".into());
            }
            Some(n)
        }
    };
    Ok(HullInput {
        owner_id,
        ship_id,
        model,
        nickname: opt(&f.nickname),
        kind,
        status,
        location,
        carried_in,
        loaner: f.loaner || loaner_for.is_some(),
        loaner_for,
        insurance_claim,
        insurance_type: opt(&f.insurance_type),
        notes: opt(&f.notes),
    })
}

/// Success from the overlay: close it, refresh the list.
async fn closed_overlay(state: &AppState, headers: &HeaderMap) -> Response {
    match body(state, query_of(headers), true).await {
        Ok(b) => html(StatusCode::OK, b),
        Err(r) => r,
    }
}

/// `POST /hulls`.
pub async fn create(Db(state): Db, _user: CurrentUser, headers: HeaderMap, body: String) -> Response {
    let mut f = form_from(&FormData::parse(&body), None);
    let hulls = repo::list_hulls(&state.pool).await.unwrap_or_default();
    match parse_form(&state, &f, &hulls).await {
        Err(e) => {
            f.error = Some(e);
            form_response(&state, f, StatusCode::UNPROCESSABLE_ENTITY).await
        }
        Ok(input) => match repo::create_hull(&state.pool, &input).await {
            Ok(_) => closed_overlay(&state, &headers).await,
            Err(e) => fail(e, "add that hull"),
        },
    }
}

/// `PUT /hulls/:id`.
pub async fn update(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, headers: HeaderMap, body: String) -> Response {
    let mut f = form_from(&FormData::parse(&body), Some(id));
    let hulls = repo::list_hulls(&state.pool).await.unwrap_or_default();
    if !hulls.iter().any(|h| h.id == id) {
        return (StatusCode::NOT_FOUND, "No such hull.").into_response();
    }
    match parse_form(&state, &f, &hulls).await {
        Err(e) => {
            f.error = Some(e);
            form_response(&state, f, StatusCode::UNPROCESSABLE_ENTITY).await
        }
        Ok(input) => match repo::update_hull(&state.pool, id, &input).await {
            Ok(()) => closed_overlay(&state, &headers).await,
            Err(e) => fail(e, "save that hull"),
        },
    }
}

/// `DELETE /hulls/:id` (the edit form's Remove).
pub async fn delete(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, headers: HeaderMap) -> Response {
    match repo::delete_hull(&state.pool, id).await {
        Ok(()) => closed_overlay(&state, &headers).await,
        Err(e) => fail(e, "remove that hull"),
    }
}

// ---------------------------------------------------------------------------
// quick actions
// ---------------------------------------------------------------------------

/// `POST /hulls/:id/status` with `action` (delivered / out / store /
/// destroyed / claim) and `location`. Answers `#fleet-body`; with
/// `reply=line` (the SALE banner's store prompt) a one-line confirmation.
pub async fn status(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, headers: HeaderMap, body: String) -> Response {
    let form = FormData::parse(&body);
    let Some(action) = Action::parse(form.get("action")) else {
        return (StatusCode::UNPROCESSABLE_ENTITY, "Pick an action.").into_response();
    };
    let location = match opt(form.get("location")) {
        Some(l) if action.takes_location() => Some(lookups::canonical(&state.pool, Kind::Terminal, &l).await.unwrap_or(l)),
        _ => None,
    };
    match repo::apply_action(&state.pool, id, action, location.as_deref()).await {
        Err(e) => return fail(e, "change that hull"),
        Ok(Err(refusal)) => return (StatusCode::CONFLICT, refusal).into_response(),
        Ok(Ok(_)) => {}
    }
    if form.get("reply") == "line" {
        let h = repo::find_hull(&state.pool, id).await.ok().flatten();
        let text = h
            .map(|h| format!("{} stored at {}", h.label(), h.location.unwrap_or_else(|| "its last stop".into())))
            .unwrap_or_default();
        return html(
            StatusCode::OK,
            format!("<div class=\"ras lit store-prompt\" id=\"store-prompt\">{}</div>", askama_escape(&text)),
        );
    }
    body_response(&state, &headers).await
}

fn askama_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `POST /hulls/:id/loaners` with `do=remove|keep` -- once the pledge is
/// delivered, its loaners go back (removed) or stay as ordinary hulls.
pub async fn loaners(Db(state): Db, _user: CurrentUser, Path(id): Path<i64>, headers: HeaderMap, body: String) -> Response {
    let form = FormData::parse(&body);
    let done = match form.get("do") {
        "remove" => repo::remove_loaners(&state.pool, id).await.map(|_| ()),
        "keep" => repo::keep_loaners(&state.pool, id).await.map(|_| ()),
        _ => return (StatusCode::UNPROCESSABLE_ENTITY, "Remove or keep?").into_response(),
    };
    if let Err(e) = done {
        return fail(e, "change those loaners");
    }
    body_response(&state, &headers).await
}

/// `FLEET ...... N HULLS · M OUT` for the splash.
pub async fn boot_line(state: &AppState) -> String {
    match repo::counts(&state.pool).await {
        Ok((n, out)) => format!("{n} HULL{} · {out} OUT", if n == 1 { "" } else { "S" }),
        Err(_) => "UNREADABLE".into(),
    }
}
