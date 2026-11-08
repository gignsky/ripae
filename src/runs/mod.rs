pub mod model;
pub mod repo;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;

use crate::auth::CurrentUser;
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

    let status = if form_error.is_some() {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::OK
    };

    let body = render(&IndexTemplate {
        display_name: user.display_name.clone(),
        handle: user.handle.clone(),
        open_runs: open.into_iter().map(|r| RunView::new(r, user.id)).collect(),
        sold_runs: sold.into_iter().map(|r| RunView::new(r, user.id)).collect(),
        committed_fmt: fmt_auec(totals.committed),
        realised_fmt: fmt_auec(totals.realised),
        form_error,
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

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Form(new_run): Form<NewRun>,
) -> Response {
    if let Some(error) = validate_new_run(&new_run) {
        return render_index(&state, &user, Some(error)).await;
    }
    if let Err(e) = repo::create(&state.pool, user.id, &new_run).await {
        tracing::error!(error = %e, "failed to create run");
        return render_index(&state, &user, Some("Could not save that run.".to_string())).await;
    }
    axum::response::Redirect::to("/").into_response()
}

async fn load_owned(state: &AppState, user: &CurrentUser, id: i64) -> Result<Run, Response> {
    match repo::find(&state.pool, id).await {
        Ok(Some(run)) if run.submitted_by == user.id => Ok(run),
        Ok(Some(_)) => {
            Err((StatusCode::FORBIDDEN, "You can only edit your own runs.").into_response())
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, "No such run.").into_response()),
        Err(e) => {
            tracing::error!(error = %e, "failed to load run");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "could not load run").into_response())
        }
    }
}

pub async fn edit_form(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    match load_owned(&state, &user, id).await {
        Ok(run) => render(&RunFormTemplate {
            run: RunView::new(&run, user.id),
            error: None,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

pub async fn update_run(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Form(edit): Form<EditRun>,
) -> Response {
    let run = match load_owned(&state, &user, id).await {
        Ok(run) => run,
        Err(resp) => return resp,
    };

    if edit.commodity.trim().is_empty() || edit.buy_terminal.trim().is_empty() || edit.scu <= 0.0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run, user.id),
                error: Some(
                    "Commodity, buy terminal, and a positive SCU are required.".to_string(),
                ),
            }),
        )
            .into_response();
    }
    if matches!(edit.status, model::Status::Stored)
        && edit.stored_at.as_deref().unwrap_or("").is_empty()
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&RunFormTemplate {
                run: RunView::new(&run, user.id),
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
                run: RunView::new(&run, user.id),
                error: Some("Could not save those changes.".to_string()),
            }),
        )
            .into_response();
    }

    match repo::find(&state.pool, id).await {
        Ok(Some(run)) => render(&RunRowTemplate {
            run: RunView::new(&run, user.id),
        })
        .into_response(),
        _ => Html("saved".to_string()).into_response(),
    }
}

pub async fn close_form(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    match load_owned(&state, &user, id).await {
        Ok(run) => render(&CloseFormTemplate {
            run: RunView::new(&run, user.id),
            error: None,
        })
        .into_response(),
        Err(resp) => resp,
    }
}

pub async fn close_run(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Form(close): Form<CloseRun>,
) -> Response {
    let run = match load_owned(&state, &user, id).await {
        Ok(run) => run,
        Err(resp) => return resp,
    };

    if close.sell_terminal.trim().is_empty() || close.sold_at.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run, user.id),
                error: Some("Sell terminal and date sold are required.".to_string()),
            }),
        )
            .into_response();
    }
    if close.sell_price_scu < 0.0 || close.scu_sold < 0.0 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run, user.id),
                error: Some("Sell price and SCU sold can't be negative.".to_string()),
            }),
        )
            .into_response();
    }

    if let Err(e) = repo::close(&state.pool, id, &close).await {
        tracing::error!(error = %e, "failed to close run");
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            render(&CloseFormTemplate {
                run: RunView::new(&run, user.id),
                error: Some("Could not record that sale.".to_string()),
            }),
        )
            .into_response();
    }

    match repo::find(&state.pool, id).await {
        Ok(Some(run)) => render(&OpenCargoRowTemplate {
            run: RunView::new(&run, user.id),
        })
        .into_response(),
        _ => Html("closed".to_string()).into_response(),
    }
}

pub async fn delete_run(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = load_owned(&state, &user, id).await {
        return resp;
    }
    if let Err(e) = repo::delete(&state.pool, id).await {
        tracing::error!(error = %e, "failed to delete run");
        return (StatusCode::INTERNAL_SERVER_ERROR, "could not delete run").into_response();
    }
    StatusCode::OK.into_response()
}
