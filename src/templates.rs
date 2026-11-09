//! Askama template structs. Pages extend `base.html`; fragments are the
//! partials htmx swaps in. Fragments carry an `oob` flag so the same
//! partial can ride along out-of-band on another response.

use askama::Template;
use axum::response::Html;

use crate::stowage::views::{
    BoardView, LedgerRow, LoadView, NewRunFormView, PlanCard, RecordView, ShareView, TotalsView,
    VoyageView,
};

/// Renders any Askama template to an axum `Html` response, falling back to
/// a plain-text error body (never a panic) if rendering itself fails.
pub fn render<T: Template>(tmpl: &T) -> Html<String> {
    Html(render_str(tmpl))
}

pub fn render_str<T: Template>(tmpl: &T) -> String {
    tmpl.render().unwrap_or_else(|e| {
        tracing::error!(error = %e, "template render failed");
        format!("template error: {e}")
    })
}

/// aUEC with thousands separators, no decimals (§4: "displayed with
/// thousands separators, no decimals").
pub fn fmt_auec(amount: f64) -> String {
    let rounded = amount.round() as i64;
    let negative = rounded < 0;
    let digits = rounded.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let grouped: String = grouped.chars().rev().collect();
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

// ---------------------------------------------------------------------------
// pages
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "changelog.html")]
pub struct ChangelogTemplate {
    /// Rendered from our own CHANGELOG.md, so it's trusted HTML.
    pub body: &'static str,
}

/// `/` -- the ledger terminal. The big panels are pre-rendered fragments so
/// the page and the htmx responses share one template each.
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub display_name: String,
    pub handle: String,
    pub totals_html: String,
    pub open_cargo_html: String,
    pub plans_html: String,
    pub ledger_html: String,
    pub new_run: NewRunFormView,
    pub commodities: Vec<String>,
    pub terminals: Vec<String>,
    pub ships: Vec<String>,
    pub today: String,
}

#[derive(Template)]
#[template(path = "board.html")]
pub struct BoardTemplate {
    pub display_name: String,
    pub handle: String,
    pub board: BoardView,
}

// ---------------------------------------------------------------------------
// fragments
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "partials/totals.html")]
pub struct TotalsFrag {
    pub t: TotalsView,
    pub oob: bool,
}

#[derive(Template)]
#[template(path = "partials/open_cargo.html")]
pub struct OpenCargoFrag {
    pub voyages: Vec<VoyageView>,
    pub oob: bool,
    /// Always false here; the included head/load partials read them.
    pub head_oob: bool,
    pub load_oob: bool,
}

#[derive(Template)]
#[template(path = "partials/voyage_head.html")]
pub struct VoyageHeadFrag {
    pub v: VoyageView,
    pub head_oob: bool,
}

#[derive(Template)]
#[template(path = "partials/load.html")]
pub struct LoadFrag {
    pub l: LoadView,
    pub load_oob: bool,
}

#[derive(Template)]
#[template(path = "partials/plans.html")]
pub struct PlansFrag {
    pub cards: Vec<PlanCard>,
    pub oob: bool,
}

#[derive(Template)]
#[template(path = "partials/ledger.html")]
pub struct LedgerFrag {
    pub rows: Vec<LedgerRow>,
    pub oob: bool,
}

#[derive(Template)]
#[template(path = "partials/sale.html")]
pub struct SaleFrag {
    pub who_upper: String,
    pub profit_fmt: String,
    pub profit_neg: bool,
    pub commodity_upper: String,
    pub scu_sold: u32,
    pub from_short: String,
    pub to_short: String,
    pub shares: Vec<ShareView>,
    pub keeps_fmt: String,
    /// Live SCU left open on the voyage after a partial close.
    pub kept_scu: u32,
}

pub struct ConfirmLine {
    pub commodity_upper: String,
    pub scu: u32,
    pub cost_fmt: String,
}

#[derive(Template)]
#[template(path = "partials/confirm_delete.html")]
pub struct ConfirmFrag {
    /// `voyage` or `load` -- decides the DELETE route.
    pub kind: &'static str,
    pub id: i64,
    pub label: String,
    pub ship_name: String,
    pub logged_by_upper: String,
    pub lines: Vec<ConfirmLine>,
    pub load_count: usize,
    pub scu: u32,
    pub committed_fmt: String,
}

#[derive(Template)]
#[template(path = "partials/verdict.html")]
pub struct VerdictFrag {
    pub r: RecordView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_thousands_separators_with_no_decimals() {
        assert_eq!(fmt_auec(1234567.8), "1,234,568");
        assert_eq!(fmt_auec(999.0), "999");
        assert_eq!(fmt_auec(-2500.0), "-2,500");
        assert_eq!(fmt_auec(0.0), "0");
    }
}
