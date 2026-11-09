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
// the shell every page shares: rail, footer, splash
// ---------------------------------------------------------------------------

/// What `base.html` needs: who's aboard, which screen is lit, and how the
/// splash should play (`gate` / `welcome` / `hidden`).
pub struct Shell {
    pub crew_name: Option<String>,
    /// `ledger` / `board` / `login` / `changelog` -- the `<main>` id.
    pub screen: &'static str,
    pub stardate: String,
    pub splash_mode: &'static str,
    /// Display name, upper-cased, for `SESSION VALID · <name>`.
    pub splash_who: String,
    /// JSON array of boot-log lines the splash types out.
    pub boot_json: String,
    pub gate_error: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// pages
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub shell: Shell,
}

#[derive(Template)]
#[template(path = "changelog.html")]
pub struct ChangelogTemplate {
    pub shell: Shell,
    /// Rendered from our own CHANGELOG.md, so it's trusted HTML.
    pub body: &'static str,
}

/// `/` -- the ledger terminal. The big panels are pre-rendered fragments so
/// the page and the htmx responses share one template each.
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub shell: Shell,
    pub totals_html: String,
    pub open_cargo_html: String,
    pub plans_html: String,
    pub ledger_html: String,
    pub new_run: NewRunFormView,
    pub commodities: Vec<String>,
    pub terminals: Vec<String>,
    pub ships: Vec<String>,
}

#[derive(Template)]
#[template(path = "board.html")]
pub struct BoardTemplate {
    pub shell: Shell,
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
    /// The bookkeeper, when someone other than the seller recorded it.
    pub kept_by_upper: Option<String>,
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
#[template(path = "partials/voyage_overlay.html")]
pub struct VoyageOverlayFrag {
    pub id: i64,
    /// `"2 loads · all sold"` and the like.
    pub summary: String,
    pub f: crate::stowage::views::VoyageFormView,
}

#[derive(Template)]
#[template(path = "partials/sale_overlay.html")]
pub struct SaleOverlayFrag {
    pub id: i64,
    pub voyage_id: i64,
    pub commodity_upper: String,
    pub scu_sold: u32,
    /// Plain number for the live preview; `_fmt` for people.
    pub buy_price_scu: String,
    pub buy_price_scu_fmt: String,
    pub f: crate::stowage::views::CloseFormView,
}

#[derive(Template)]
#[template(path = "partials/confirm_write_off.html")]
pub struct ConfirmWriteOffFrag {
    pub id: i64,
    pub voyage_id: i64,
    pub commodity_upper: String,
    pub by_upper: String,
    /// Crates still in play (sound or glitched), their SCU and buy cost.
    pub crates: usize,
    pub scu: u32,
    pub cost_fmt: String,
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
