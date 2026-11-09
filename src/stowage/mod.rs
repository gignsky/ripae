//! 0.2 "Stowage": voyages carrying cargo loads made of crates. `model` holds
//! the types and every derived figure; `repo` the SQL. Handlers follow in
//! roll 7 step 7.4; for now `/` shows the totals off the new schema.

pub mod model;
pub mod repo;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::auth::CurrentUser;
use crate::templates::{fmt_auec, render, IndexTemplate};
use crate::AppState;
use model::Totals;

pub async fn index(State(state): State<AppState>, user: CurrentUser) -> Response {
    let voyages = match repo::list_voyages(&state.pool).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "failed to list voyages");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not load the ledger").into_response();
        }
    };
    let totals = Totals::from_voyages(&voyages);
    render(&IndexTemplate {
        display_name: user.display_name,
        committed_fmt: fmt_auec(totals.committed),
        realised_fmt: fmt_auec(totals.realised),
        open_loads: totals.open_loads,
        open_scu: totals.open_scu,
    })
    .into_response()
}
