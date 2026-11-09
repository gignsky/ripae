use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    InTransit,
    Stored,
    Sold,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::InTransit => "in_transit",
            Status::Stored => "stored",
            Status::Sold => "sold",
        }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self, Status::Sold)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Status {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_transit" => Ok(Status::InTransit),
            "stored" => Ok(Status::Stored),
            "sold" => Ok(Status::Sold),
            other => Err(format!("unknown run status: {other}")),
        }
    }
}

/// A commodity run, purchase leg always populated, sale leg populated once
/// `status == Sold`.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: i64,
    /// `users.display_name` of whoever logged the run, joined in on every read.
    pub submitted_by_name: String,
    pub status: Status,

    pub bought_at: String,
    pub commodity: String,
    pub scu: f64,
    pub buy_terminal: String,
    pub buy_price_scu: f64,

    pub destination: Option<String>,
    pub stored_at: Option<String>,
    pub ship: Option<String>,

    pub sold_at: Option<String>,
    pub sell_terminal: Option<String>,
    pub sell_price_scu: Option<f64>,
    pub scu_sold: Option<f64>,

    pub notes: Option<String>,
}

impl Run {
    /// `cost = scu * buy_price_scu` -- always known.
    pub fn cost(&self) -> f64 {
        self.scu * self.buy_price_scu
    }

    /// `revenue = scu_sold * sell_price_scu` -- `None` until sold.
    pub fn revenue(&self) -> Option<f64> {
        Some(self.scu_sold? * self.sell_price_scu?)
    }

    /// `profit = revenue - cost` -- `None` until sold.
    pub fn profit(&self) -> Option<f64> {
        Some(self.revenue()? - self.cost())
    }

    /// `margin = profit / cost`, `0` when `cost == 0` -- `None` until sold.
    pub fn margin(&self) -> Option<f64> {
        let cost = self.cost();
        let profit = self.profit()?;
        if cost == 0.0 {
            Some(0.0)
        } else {
            Some(profit / cost)
        }
    }
}

/// The "aUEC afloat" / "realised profit" totals shown on the Ledger page.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Totals {
    /// Σ cost over open (unsold) runs.
    pub committed: f64,
    /// Σ profit over sold runs.
    pub realised: f64,
}

impl Totals {
    pub fn from_runs<'a>(runs: impl IntoIterator<Item = &'a Run>) -> Self {
        let mut totals = Totals::default();
        for run in runs {
            match run.profit() {
                Some(profit) => totals.realised += profit,
                None => totals.committed += run.cost(),
            }
        }
        totals
    }
}

/// Fields collected by the "new run" form (purchase leg + optional intent).
#[derive(Debug, Clone, Deserialize)]
pub struct NewRun {
    pub bought_at: String,
    pub commodity: String,
    pub scu: f64,
    pub buy_terminal: String,
    pub buy_price_scu: f64,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub ship: Option<String>,
}

/// Fields collected by the "close run" form -- records the sale.
#[derive(Debug, Clone, Deserialize)]
pub struct CloseRun {
    pub sold_at: String,
    pub sell_terminal: String,
    pub sell_price_scu: f64,
    pub scu_sold: f64,
}

/// Fields collected by the edit form -- the full editable set, status
/// included (so a stored load can go back to `in_transit`, per §4).
#[derive(Debug, Clone, Deserialize)]
pub struct EditRun {
    pub status: Status,
    pub bought_at: String,
    pub commodity: String,
    pub scu: f64,
    pub buy_terminal: String,
    pub buy_price_scu: f64,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub stored_at: Option<String>,
    #[serde(default)]
    pub ship: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Today's (UTC) date, the server-side default for date
/// fields; the page swaps in the viewer's local date.
pub fn today() -> time::Date {
    time::OffsetDateTime::now_utc().date()
}

/// Accepts a real calendar date as `YYYY-MM-DD` (what `<input type="date">`
/// submits) and returns it normalised. Years past next year are refused --
/// those are almost always in-game SC dates (real year + 930).
pub fn parse_date(raw: &str, today: time::Date) -> Result<String, String> {
    let bad = || format!("\"{}\" isn't a date -- use YYYY-MM-DD.", raw.trim());
    let mut parts = raw.trim().splitn(3, '-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(bad());
    };
    let year: i32 = y.parse().map_err(|_| bad())?;
    let month: u8 = m.parse().map_err(|_| bad())?;
    let day: u8 = d.parse().map_err(|_| bad())?;
    let month = time::Month::try_from(month).map_err(|_| bad())?;
    let date = time::Date::from_calendar_date(year, month, day).map_err(|_| bad())?;
    if date.year() > today.year() + 1 {
        return Err(format!(
            "{date} looks like an in-game date -- enter the real-world date instead."
        ));
    }
    Ok(date.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_run(scu: f64, buy_price_scu: f64) -> Run {
        Run {
            id: 1,
            submitted_by_name: "Lord Gig".into(),
            status: Status::InTransit,
            bought_at: "2026-01-01".into(),
            commodity: "Laranite".into(),
            scu,
            buy_terminal: "ArcCorp Mining Area 141".into(),
            buy_price_scu,
            destination: None,
            stored_at: None,
            ship: None,
            sold_at: None,
            sell_terminal: None,
            sell_price_scu: None,
            scu_sold: None,
            notes: None,
        }
    }

    fn sell(mut run: Run, scu_sold: f64, sell_price_scu: f64) -> Run {
        run.status = Status::Sold;
        run.sold_at = Some("2026-01-02".into());
        run.sell_terminal = Some("Everus Harbor".into());
        run.scu_sold = Some(scu_sold);
        run.sell_price_scu = Some(sell_price_scu);
        run
    }

    #[test]
    fn ordinary_sale_profits() {
        let run = sell(open_run(100.0, 10.0), 100.0, 15.0);
        assert_eq!(run.cost(), 1000.0);
        assert_eq!(run.revenue(), Some(1500.0));
        assert_eq!(run.profit(), Some(500.0));
        assert_eq!(run.margin(), Some(0.5));
    }

    #[test]
    fn partial_sale_only_counts_what_sold() {
        // Bought 100 SCU, only 60 actually sold (seizure/loss of the rest).
        let run = sell(open_run(100.0, 10.0), 60.0, 15.0);
        assert_eq!(run.cost(), 1000.0);
        assert_eq!(run.revenue(), Some(900.0));
        assert_eq!(run.profit(), Some(-100.0));
    }

    #[test]
    fn total_loss_is_zero_revenue() {
        let run = sell(open_run(100.0, 10.0), 0.0, 15.0);
        assert_eq!(run.revenue(), Some(0.0));
        assert_eq!(run.profit(), Some(-1000.0));
    }

    #[test]
    fn zero_cost_edge_case_margin_is_zero_not_nan() {
        let run = sell(open_run(100.0, 0.0), 100.0, 15.0);
        assert_eq!(run.cost(), 0.0);
        assert_eq!(run.margin(), Some(0.0));
    }

    #[test]
    fn open_run_has_no_profit_yet() {
        let run = open_run(100.0, 10.0);
        assert_eq!(run.revenue(), None);
        assert_eq!(run.profit(), None);
        assert_eq!(run.margin(), None);
    }

    #[test]
    fn totals_split_open_vs_sold_correctly() {
        let open = open_run(50.0, 20.0); // cost 1000, open -> committed
        let sold = sell(open_run(100.0, 10.0), 100.0, 15.0); // profit 500 -> realised
        let totals = Totals::from_runs([&open, &sold]);
        assert_eq!(totals.committed, 1000.0);
        assert_eq!(totals.realised, 500.0);
    }

    #[test]
    fn status_round_trips_through_strings() {
        for status in [Status::InTransit, Status::Stored, Status::Sold] {
            let s = status.to_string();
            assert_eq!(s.parse::<Status>().unwrap(), status);
        }
    }

    #[test]
    fn parse_date_accepts_real_dates_and_normalises() {
        let today = time::Date::from_calendar_date(2026, time::Month::November, 8).unwrap();
        assert_eq!(parse_date(" 2026-11-08 ", today), Ok("2026-11-08".to_string()));
        assert_eq!(parse_date("2026-1-5", today), Ok("2026-01-05".to_string()));
        assert_eq!(parse_date("2027-12-31", today), Ok("2027-12-31".to_string()));
    }

    #[test]
    fn parse_date_rejects_garbage_impossible_and_sc_dates() {
        let today = time::Date::from_calendar_date(2026, time::Month::November, 8).unwrap();
        assert!(parse_date("", today).is_err());
        assert!(parse_date("yesterday", today).is_err());
        assert!(parse_date("2026-02-30", today).is_err());
        let sc = parse_date("2956-11-08", today).unwrap_err();
        assert!(sc.contains("in-game"), "{sc}");
    }
}
