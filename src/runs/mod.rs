pub mod model;
pub mod repo;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;

use crate::auth::CurrentUser;
use crate::lookups::{self, Kind};
use crate::templates::{
    fmt_auec, render, CloseFormTemplate, IndexTemplate, OpenCargoRowTemplate, RunFormTemplate,
    RunRowTemplate, RunView,
};
use crate::AppState;
use model::{CloseRun, EditRun, NewRun, Run, Totals};

pub async fn index(State(state): State<AppState>, user: CurrentUser) -> Response {
    render_index(&state, &user, None).await
}

async fn render_index(
    state: &AppState,
    user: &CurrentUser,
    form_error: Option<String>,
) -> Response {
    let all = match repo::list_all(&state.pool).await {
        Ok(all) => all,
        Err(e) => {
            tracing::error!(error = %e, "failed to list runs");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not load runs").into_response();
        }
    };

    let mut open: Vec<&Run> = all.iter().filter(|r| r.status.is_open()).collect();
    open.sort_by(|a, b| a.bought_at.cmp(&b.bought_at)); // oldest first: oldest forgotten cargo first
    let mut sold: Vec<&Run> = all.iter().filter(|r| !r.status.is_open()).collect();
    sold.sort_by(|a, b| b.sold_at.cmp(&a.sold_at)); // newest first

    let totals = Totals::from_runs(all.iter());

    // Pick-lists and defaults only enrich the form; if one fails to load,
    // log it and render the page without it rather than failing outright.
    let list = |kind: Kind| async move {
        lookups::names(&state.pool, kind).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, list = kind.label(), "failed to load pick-list");
            Vec::new()
        })
    };
    let commodities = list(Kind::Commodity).await;
    let terminals = list(Kind::Terminal).await;
    let ships = list(Kind::Ship).await;
    let default_buy_terminal = repo::latest_sell_terminal(&state.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to load latest sell terminal");
            None
        })
        .unwrap_or_default();
    let default_ship = repo::latest_ship(&state.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to load latest ship");
            None
        })
        .unwrap_or_default();

    let status = if form_error.is_some() {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };

    let body = render(&IndexTemplate {
        display_name: user.display_name.clone(),
        handle: user.handle.clone(),
        open_runs: open.into_iter().map(|r| RunView::new(r)).collect(),
        sold_runs: sold.into_iter().map(|r| RunView::new(r)).collect(),
        committed_fmt: fmt_auec(totals.committed),
        realised_fmt: fmt_auec(totals.realised),
        form_error,
        commodities,
        terminals,
        ships,
        default_buy_terminal,
        default_ship,
    });
    (status, body).into_response()
}

fn validate_new_run(new_run: &NewRun) -> Option<String> {
    if new_run.commodity.trim().is_empty() {
        return Some("Commodity is required.".to_string());
    }
    if new_run.buy_terminal.trim().is_empty() {
        return Some("Buy terminal is required.".to_string());
    }
    if new_run.scu <= 0.0 {
        return Some("SCU must be greater than zero.".to_string());
    }
    if new_run.buy_price_scu < 0.0 {
        return Some("Buy price can't be negative.".to_string());
    }
    if new_run.bought_at.trim().is_empty() {
        return Some("Bought-at date is required.".to_string());
    }
    None
}

/// Swaps a submitted name for its pick-list spelling, or says why not.
async fn pick(state: &AppState, kind: Kind, field: &str, raw: &str) -> Result<String, String> {
    match lookups::canonical(&state.pool, kind, raw).await {
        Ok(Some(name)) => Ok(name),
        Ok(None) => Err(format!(
            "{field} \"{}\" isn't on the {} list -- pick one from the suggestions.",
            raw.trim(),
            kind.label()
        )),
        Err(e) => {
            tracing::error!(error = %e, list = kind.label(), "failed to check pick-list");
            Err(format!("Could not check the {} list.", kind.label()))
        }
    }
}

/// As `pick`, for optional fields: blank means "not set", never `""`.
async fn pick_opt(
    state: &AppState,
    kind: Kind,
    field: &str,
    raw: Option<String>,
) -> Result<Option<String>, String> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(value) => pick(state, kind, field, value).await.map(Some),
    }
}

async fn pick_new_run(state: &AppState, new_run: &mut NewRun) -> Result<(), String> {
    new_run.commodity = pick(state, Kind::Commodity, "Commodity", &new_run.commodity).await?;
    new_run.buy_terminal =
        pick(state, Kind::Terminal, "Buy terminal", &new_run.buy_terminal).await?;
    new_run.destination =
        pick_opt(state, Kind::Terminal, "Destination", new_run.destination.take()).await?;
    new_run.ship = pick_opt(state, Kind::Ship, "Ship", new_run.ship.take()).await?;
    Ok(())
}

async fn pick_edit(state: &AppState, edit: &mut EditRun) -> Result<(), String> {
    edit.commodity = pick(state, Kind::Commodity, "Commodity", &edit.commodity).await?;
    edit.buy_terminal = pick(state, Kind::Terminal, "Buy terminal", &edit.buy_terminal).await?;
    edit.destination =
        pick_opt(state, Kind::Terminal, "Destination", edit.destination.take()).await?;
    edit.stored_at = pick_opt(state, Kind::Terminal, "Stored at", edit.stored_at.take()).await?;
    edit.ship = pick_opt(state, Kind::Ship, "Ship", edit.ship.take()).await?;
    edit.notes = edit.notes.take().filter(|n| !n.trim().is_empty());
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Form(mut new_run): Form<NewRun>,
) -> Response {
    if let Some(error) = validate_new_run(&new_run) {
        return render_index(&state, &user, Some(error)).await;
    }
    if let Err(error) = pick_new_run(&state, &mut new_run).await {
        return render_index(&state, &user, Some(error)).await;
    }
    if let Err(e) = repo::create(&state.pool, user.id, &new_run).await {
        tracing::error!(error = %e, "failed to create run");
        return render_index(&state, &user, Some("Could not save that run.".to_string())).await;
    }
    axum::response::Redirect::to("/").into_response()
}

/// The ledger is shared crew-wide: any logged-in user may edit, close, or
/// delete any run, so cargo one of you bought the other can sell.
async fn load_run(state: &AppState, id: i64) -> Result<Run, Response> {
    match repo::find(&state.pool, id).await {
        Ok(Some(run)) => Ok(run),
        Ok(None) => Err((StatusCode::NOT_FOUND, "No such run.").into_response()),
        Err(e) => {
            tracing::error!(error = %e, "failed to load run");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "could not load run").into_response())
        }
    }
}

pub async fn edit_form(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    match load_run(&state, id).await {
        Ok(run) => render(&RunFormTemplate {
            run: RunView::new(&run),
            error: None,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

pub async fn update_run(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<i64>,
    Form(mut edit): Form<EditRun>,
) -> Response {
    let run = match load_run(&state, id).await {
        Ok(run) => run,
        Err(resp) => return resp,
    };

    if edit.commodity.trim().is_empty() || edit.buy_terminal.trim().is_empty() || edit.scu <= 0.0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run),
                error: Some(
                    "Commodity, buy terminal, and a positive SCU are required.".to_string(),
                ),
            }),
        )
            .into_response();
    }
    if let Err(error) = pick_edit(&state, &mut edit).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run),
                error: Some(error),
            }),
        )
            .into_response();
    }
    if matches!(edit.status, model::Status::Stored) && edit.stored_at.is_none() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run),
                error: Some("Stored cargo needs a stored-at station.".to_string()),
            }),
        )
            .into_response();
    }

    if let Err(e) = repo::update(&state.pool, id, &edit).await {
        tracing::error!(error = %e, "failed to update run");
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run),
                error: Some("Could not save those changes.".to_string()),
            }),
        )
            .into_response();
    }

    match repo::find(&state.pool, id).await {
        Ok(Some(run)) => render(&RunRowTemplate {
            run: RunView::new(&run),
        })
        .into_response(),
        _ => Html("saved".to_string()).into_response(),
    }
}

pub async fn close_form(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    match load_run(&state, id).await {
        Ok(run) => render(&CloseFormTemplate {
            run: RunView::new(&run),
            error: None,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

pub async fn close_run(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<i64>,
    Form(mut close): Form<CloseRun>,
) -> Response {
    let run = match load_run(&state, id).await {
        Ok(run) => run,
        Err(resp) => return resp,
    };

    if close.sell_terminal.trim().is_empty() || close.sold_at.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run),
                error: Some("Sell terminal and date sold are required.".to_string()),
            }),
        )
            .into_response();
    }
    if close.sell_price_scu < 0.0 || close.scu_sold < 0.0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run),
                error: Some("Sell price and SCU sold can't be negative.".to_string()),
            }),
        )
            .into_response();
    }
    close.sell_terminal =
        match pick(&state, Kind::Terminal, "Sell terminal", &close.sell_terminal).await {
            Ok(name) => name,
            Err(error) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    render(&CloseFormTemplate {
                        run: RunView::new(&run),
                        error: Some(error),
                    }),
                )
                    .into_response();
            }
        };

    if let Err(e) = repo::close(&state.pool, id, &close).await {
        tracing::error!(error = %e, "failed to close run");
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run),
                error: Some("Could not record that sale.".to_string()),
            }),
        )
            .into_response();
    }

    match repo::find(&state.pool, id).await {
        Ok(Some(run)) => render(&OpenCargoRowTemplate {
            run: RunView::new(&run),
        })
        .into_response(),
        _ => Html("closed".to_string()).into_response(),
    }
}

pub async fn delete_run(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = load_run(&state, id).await {
        return resp;
    }
    if let Err(e) = repo::delete(&state.pool, id).await {
        tracing::error!(error = %e, "failed to delete run");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not delete run").into_response();
    }
    StatusCode::OK.into_response()
}
