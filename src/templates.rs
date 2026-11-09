use askama::Template;
use axum::response::Html;

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

/// Interim ledger page for roll 7 step 7.3: the 0.2 schema is in and the
/// `runs` module is gone; the real screens arrive in 7.4.
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub display_name: String,
    pub committed_fmt: String,
    pub realised_fmt: String,
    pub open_loads: usize,
    pub open_scu: u32,
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
