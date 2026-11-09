use askama::Template;
use axum::response::Html;

use crate::runs::model::Run;

/// Renders any Askama template to an axum `Html` response, falling back to
/// a plain-text error body (never a panic) if rendering itself fails.
pub fn render<T: Template>(tmpl: &T) -> Html<String> {
    Html(
        tmpl.render()
            .unwrap_or_else(|e| format!("template error: {e}")),
    )
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

/// Flat, template-friendly view of a `Run` -- all display formatting
/// (aUEC amounts, submitter name) is resolved in Rust so the templates only
/// ever do plain field access and `{% if %}` on booleans/options.
pub struct RunView {
    pub id: i64,
    pub submitted_by_name: String,
    pub status_label: &'static str,
    pub bought_at: String,
    pub commodity: String,
    pub scu: f64,
    pub buy_terminal: String,
    pub buy_price_scu: f64,
    pub cost_fmt: String,
    pub destination: Option<String>,
    pub stored_at: Option<String>,
    pub ship: Option<String>,
    pub notes: Option<String>,
    /// Plain (never-`None`) counterparts of the above, for pre-filling form
    /// `value=""` attributes without chaining `Option` methods in templates.
    pub destination_input: String,
    pub stored_at_input: String,
    pub ship_input: String,
    pub notes_input: String,
    pub sold_at: Option<String>,
    pub sell_terminal: Option<String>,
    pub sell_price_scu: Option<f64>,
    pub scu_sold: Option<f64>,
    pub profit_fmt: Option<String>,
    pub margin_pct_fmt: Option<String>,
}

impl RunView {
    pub fn new(run: &Run) -> Self {
        Self {
            id: run.id,
            submitted_by_name: run.submitted_by_name.clone(),
            status_label: run.status.as_str(),
            bought_at: run.bought_at.clone(),
            commodity: run.commodity.clone(),
            scu: run.scu,
            buy_terminal: run.buy_terminal.clone(),
            buy_price_scu: run.buy_price_scu,
            cost_fmt: fmt_auec(run.cost()),
            destination: run.destination.clone(),
            stored_at: run.stored_at.clone(),
            ship: run.ship.clone(),
            notes: run.notes.clone(),
            destination_input: run.destination.clone().unwrap_or_default(),
            stored_at_input: run.stored_at.clone().unwrap_or_default(),
            ship_input: run.ship.clone().unwrap_or_default(),
            notes_input: run.notes.clone().unwrap_or_default(),
            sold_at: run.sold_at.clone(),
            sell_terminal: run.sell_terminal.clone(),
            sell_price_scu: run.sell_price_scu,
            scu_sold: run.scu_sold,
            profit_fmt: run.profit().map(fmt_auec),
            margin_pct_fmt: run.margin().map(|m| format!("{:.0}", m * 100.0)),
        }
    }
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub display_name: String,
    pub handle: String,
    pub open_runs: Vec<RunView>,
    pub sold_runs: Vec<RunView>,
    pub committed_fmt: String,
    pub realised_fmt: String,
    pub form_error: Option<String>,
    /// Pick-lists, rendered once as `<datalist>`s that every form on the
    /// page (including htmx-swapped edit/close rows) draws from.
    pub commodities: Vec<String>,
    pub terminals: Vec<String>,
    pub ships: Vec<String>,
    /// New-run defaults: the crew's last delivery point and last ship.
    pub default_buy_terminal: String,
    pub default_ship: String,
}

#[derive(Template)]
#[template(path = "partials/run_row.html")]
pub struct RunRowTemplate {
    pub run: RunView,
}

#[derive(Template)]
#[template(path = "partials/open_cargo_row.html")]
pub struct OpenCargoRowTemplate {
    pub run: RunView,
}

#[derive(Template)]
#[template(path = "partials/run_form.html")]
pub struct RunFormTemplate {
    pub run: RunView,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "partials/close_form.html")]
pub struct CloseFormTemplate {
    pub run: RunView,
    pub error: Option<String>,
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
