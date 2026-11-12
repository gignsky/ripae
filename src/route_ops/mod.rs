//! Route Ops (`/route-ops`): the phase-aware voyage card from
//! `RIPAE-0.3-ROUTE-OPS.md`, alongside -- not instead of -- the Ledger's
//! Open Cargo and Stowage Plan panels. Everything here is read-only
//! re-presentation: the mutations the card offers are the existing
//! voyage/load endpoints (`stowage::mod`) plus one new one, `retreat`, and
//! the close form opened in `#overlay` instead of an Open Cargo row.

pub mod views;

use askama::Template;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::auth::{CurrentUser, SplashMode};
use crate::sandbox::Db;
use crate::stowage::{self, model::Voyage};
use crate::templates::{render_str, Shell};
use views::{route_ops_cards, VoyageCardView};

#[derive(Template)]
#[template(path = "route_ops.html")]
pub struct RouteOpsTemplate {
    pub shell: Shell,
    pub cards: Vec<VoyageCardView>,
    /// Always false here; `route_ops_body.html` is included inline. Exists
    /// so the included partial's own `{% if oob %}` resolves.
    pub oob: bool,
}

#[derive(Template)]
#[template(path = "partials/route_ops_body.html")]
pub struct RouteOpsBodyFrag {
    pub cards: Vec<VoyageCardView>,
    pub oob: bool,
}

/// `GET /route-ops`.
pub async fn page(Db(state): Db, user: CurrentUser) -> Response {
    let voyages = match stowage::repo::list_voyages(&state.pool).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "route-ops: failed to list voyages");
            return (StatusCode::INTERNAL_SERVER_ERROR, "could not load voyages").into_response();
        }
    };
    (
        StatusCode::OK,
        axum::response::Html(render_str(&RouteOpsTemplate {
            shell: stowage::shell(&state, Some(&user), "route-ops", SplashMode::Hidden, None).await,
            cards: route_ops_cards(&voyages),
            oob: false,
        })),
    )
        .into_response()
}

/// Every open voyage's card, out of band -- appended to `stowage::mod`'s
/// `oob_common` so any mutation anywhere keeps this page's cards in step.
/// Harmless where `#route-ops-cards` doesn't exist (every other page): htmx
/// drops an out-of-band swap with no matching id.
pub fn body_oob(voyages: &[Voyage]) -> String {
    render_str(&RouteOpsBodyFrag { cards: route_ops_cards(voyages), oob: true })
}
