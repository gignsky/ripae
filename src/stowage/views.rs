//! Flat, template-friendly views of the model. Every figure is formatted
//! here so the Askama templates only do field access, `{% if %}` and
//! `{% for %}`; nothing is computed in a template.

use super::model::{
    cluster_unassigned, format_boxes, sc_cargo_link, standings, stowage_order, Crate, Load,
    LoadStatus, ServiceRecord, Share, SortKey, Standing, Totals, User, Voyage, Whereabouts,
};
use crate::templates::fmt_auec;

// ---------------------------------------------------------------------------
// formatting helpers
// ---------------------------------------------------------------------------

/// `+748,800` / `−118,400` (typographic minus, as the mock has it).
pub fn fmt_signed(amount: f64) -> String {
    let body = fmt_auec(amount.abs());
    if amount.round() < 0.0 {
        format!("−{body}")
    } else {
        format!("+{body}")
    }
}

/// Prices per SCU with two decimals: `8.90`.
pub fn fmt_price(p: f64) -> String {
    let whole = fmt_auec(p.trunc().abs());
    let cents = ((p.abs() - p.abs().trunc()) * 100.0).round() as i64;
    let sign = if p < 0.0 { "-" } else { "" };
    if cents >= 100 {
        return format!("{sign}{}.00", fmt_auec(p.abs().round()));
    }
    format!("{sign}{whole}.{cents:02}")
}

/// `+17.1%` / `−100.0%` from a ratio.
pub fn fmt_margin(ratio: f64) -> String {
    let pct = ratio * 100.0;
    let sign = if pct < 0.0 { "−" } else { "+" };
    format!("{sign}{:.1}%", pct.abs())
}

/// `2026-11-07` → `11-07`; anything else passes through.
pub fn short_date(iso: &str) -> String {
    if iso.len() == 10 && iso.as_bytes()[4] == b'-' {
        iso[5..].to_string()
    } else {
        iso.to_string()
    }
}

/// The mock's short station names: `Shubin Mining Facility SCD-1` →
/// `Shubin SCD-1`, `ArcCorp Mining Area 141` → `ArcCorp 141`.
pub fn short_terminal(name: &str) -> String {
    name.replace("Mining Facility ", "").replace("Mining Area ", "")
}

fn where_css(w: Whereabouts) -> &'static str {
    match w {
        Whereabouts::AwaitingPickup => "pickup",
        Whereabouts::Aboard => "aboard",
        Whereabouts::Stored => "stored",
    }
}

fn where_label(w: Whereabouts) -> &'static str {
    match w {
        Whereabouts::AwaitingPickup => "awaiting pickup",
        Whereabouts::Aboard => "aboard",
        Whereabouts::Stored => "stored",
    }
}

// ---------------------------------------------------------------------------
// crates, shares, loads, voyages
// ---------------------------------------------------------------------------

pub struct CrateView {
    pub id: i64,
    /// 1-based position in the load, for `#n`.
    pub n: usize,
    pub size: u32,
    pub condition: &'static str,
    pub where_css: &'static str,
    pub where_label: &'static str,
    pub stored_at: String,
    pub is_lost: bool,
    pub is_sold: bool,
    pub is_glitched: bool,
    pub is_stored: bool,
    pub sellable: bool,
}

impl CrateView {
    pub fn new(n: usize, c: &Crate) -> Self {
        Self {
            id: c.id,
            n,
            size: c.size_scu,
            condition: c.condition.as_str(),
            where_css: where_css(c.whereabouts),
            where_label: where_label(c.whereabouts),
            stored_at: c.stored_at.clone().unwrap_or_default(),
            is_lost: c.is_lost(),
            is_sold: c.is_sold(),
            is_glitched: c.condition == super::model::Condition::Glitched,
            is_stored: c.whereabouts == Whereabouts::Stored,
            sellable: c.is_sellable(),
        }
    }
}

pub struct ShareView {
    pub to_name: String,
    pub paid_to_name: String,
    pub forfeited: bool,
    pub terms: String,
    pub reason: String,
    pub paid_fmt: String,
}

impl ShareView {
    pub fn new(s: &Share) -> Self {
        let terms = match s.kind {
            super::model::ShareKind::Pct => format!("{}% of profit", trim_num(s.amount)),
            super::model::ShareKind::Flat => "flat".to_string(),
        };
        Self {
            to_name: s.to_user_name.to_uppercase(),
            paid_to_name: s.paid_to_name.to_uppercase(),
            forfeited: s.paid_to != s.to_user,
            terms,
            reason: s.reason.clone().unwrap_or_default(),
            paid_fmt: fmt_auec(s.paid_auec),
        }
    }
}

/// `25` rather than `25.0` for percentages typed as whole numbers.
pub fn trim_num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

pub fn to_cents(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// What sits under a load row: nothing, the crate manifest, an edit form or
/// the close form.
pub enum LoadExtra {
    None,
    Manifest,
    Edit(LoadFormView),
    Close(CloseFormView),
}

pub struct LoadView {
    pub id: i64,
    pub voyage_id: i64,
    pub commodity_upper: String,
    pub scu: u32,
    pub scu_lost: u32,
    pub boxes: Vec<(u32, u32)>,
    pub lost_count: usize,
    pub glitched_count: usize,
    pub buy_terminal: String,
    pub buy_terminal_short: String,
    pub status_label: &'static str,
    pub status_css: &'static str,
    pub stop_no: u32,
    pub destination_label: String,
    pub where_now: String,
    pub cost_fmt: String,
    pub written_off_fmt: String,
    pub has_write_off: bool,
    pub by_upper: String,
    pub buy_price_scu_fmt: String,
    pub notes: String,
    /// First station any crate is stored at, for pre-filling forms.
    pub stored_at: String,
    pub crates: Vec<CrateView>,
    // sale leg
    pub sold_at_short: String,
    pub sell_terminal_short: String,
    pub sell_price_scu_fmt: String,
    pub scu_sold: u32,
    pub profit_fmt: String,
    pub profit_neg: bool,
    pub margin_fmt: String,
    pub shares: Vec<ShareView>,
    pub extra: LoadExtra,
}

impl LoadView {
    pub fn new(l: &Load, hull: Option<&str>) -> Self {
        let status = l.status();
        let stored_at = l
            .crates
            .iter()
            .find_map(|c| c.stored_at.clone())
            .unwrap_or_default();
        Self {
            id: l.id,
            voyage_id: l.voyage_id,
            commodity_upper: l.commodity.to_uppercase(),
            scu: l.scu(),
            scu_lost: l.scu_lost(),
            boxes: l.boxes(),
            lost_count: l.lost_count(),
            glitched_count: l.glitched_count(),
            buy_terminal: l.buy_terminal.clone(),
            buy_terminal_short: if l.buy_terminal.is_empty() { "—".into() } else { short_terminal(&l.buy_terminal) },
            status_label: status.label(),
            status_css: status.css(),
            stop_no: l.stop_no,
            destination_label: l
                .destination
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| "undecided".into()),
            where_now: l.where_now(hull),
            cost_fmt: fmt_auec(l.cost()),
            written_off_fmt: fmt_auec(l.written_off()),
            has_write_off: l.scu_lost() > 0,
            by_upper: l.submitted_by_name.to_uppercase(),
            buy_price_scu_fmt: fmt_price(l.buy_price_scu),
            notes: l.notes.clone().unwrap_or_default(),
            stored_at,
            crates: l
                .crates
                .iter()
                .enumerate()
                .map(|(i, c)| CrateView::new(i + 1, c))
                .collect(),
            sold_at_short: l.sold_at.as_deref().map(short_date).unwrap_or_default(),
            sell_terminal_short: l.sell_terminal.as_deref().map(short_terminal).unwrap_or_default(),
            sell_price_scu_fmt: l.sell_price_scu.map(fmt_price).unwrap_or_default(),
            scu_sold: l.scu_sold(),
            profit_fmt: l.profit().map(fmt_signed).unwrap_or_default(),
            profit_neg: l.profit().map_or(false, |p| p < 0.0),
            margin_fmt: l.margin().map(fmt_margin).unwrap_or_default(),
            shares: l.shares.iter().map(ShareView::new).collect(),
            extra: LoadExtra::None,
        }
    }
}

/// What sits under a voyage header: nothing, the merge chooser, the
/// add-load form or the voyage edit form.
pub enum VoyageExtra {
    None,
    Merge(Vec<MergeChoice>),
    AddLoad(LoadFormView),
    Edit(VoyageFormView),
}

pub struct MergeChoice {
    pub id: i64,
    pub label: String,
}

pub struct VoyageView {
    pub id: i64,
    pub label: String,
    pub date_short: String,
    pub ship_name: String,
    pub logged_by_upper: String,
    pub load_count: usize,
    pub stop_count: usize,
    pub notes: String,
    /// Open loads only (sold ones live on the Ledger).
    pub loads: Vec<LoadView>,
    pub extra: VoyageExtra,
}

impl VoyageView {
    pub fn new(v: &Voyage) -> Self {
        let hull = v.hull_label();
        let open: Vec<&Load> = v.open_loads().collect();
        Self {
            id: v.id,
            label: format!("Voyage {}", v.id),
            date_short: short_date(&v.bought_at),
            ship_name: v.ship.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "—".into()),
            logged_by_upper: v.logged_by_name.to_uppercase(),
            load_count: open.len(),
            stop_count: v.stop_count(),
            notes: v.notes.clone().unwrap_or_default(),
            loads: open.iter().map(|l| LoadView::new(l, hull.as_deref())).collect(),
            extra: VoyageExtra::None,
        }
    }
}

// ---------------------------------------------------------------------------
// forms (also carry submitted values back on a 422)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct LoadFormView {
    pub commodity: String,
    pub boxes: String,
    pub scu: String,
    pub price_mode: String,
    pub price_scu: String,
    pub price_lot: String,
    /// "balance" price mode only; never stored -- the load keeps the lot price.
    pub balance_before: String,
    pub balance_after: String,
    pub terminal: String,
    pub stop_no: String,
    pub destination: String,
    /// `awaiting_pickup` / `aboard` / `stored` / `` (leave crates alone).
    pub status: String,
    pub stored_at: String,
    pub notes: String,
    /// Render the status / stored-at fields (edit and add-load; New Run
    /// takes one status for the whole voyage).
    pub show_status: bool,
    /// Add Load only: who bought it (user id); the crew list to pick from.
    pub bought_by: String,
    pub users: Vec<User>,
    pub error: Option<String>,
}

impl LoadFormView {
    pub fn blank(terminal: &str) -> Self {
        Self {
            price_mode: "scu".into(),
            stop_no: "1".into(),
            terminal: terminal.into(),
            status: "aboard".into(),
            ..Default::default()
        }
    }

    pub fn from_load(l: &Load) -> Self {
        let status = match l.status() {
            LoadStatus::AwaitingPickup => "awaiting_pickup",
            LoadStatus::InTransit => "aboard",
            LoadStatus::Stored => "stored",
            _ => "",
        };
        Self {
            commodity: l.commodity.clone(),
            boxes: format_boxes(&l.boxes()),
            scu: l.scu().to_string(),
            price_mode: l.price_mode.as_str().into(),
            // A per-lot price leaves a derived per-SCU price like 333.333…;
            // show both to the cent so the inputs hold a sane figure.
            price_scu: trim_num(to_cents(l.buy_price_scu)),
            price_lot: l.buy_price_lot.map(|p| trim_num(to_cents(p))).unwrap_or_default(),
            balance_before: String::new(),
            balance_after: String::new(),
            terminal: l.buy_terminal.clone(),
            stop_no: l.stop_no.to_string(),
            destination: l.destination.clone().unwrap_or_default(),
            status: status.into(),
            stored_at: l.crates.iter().find_map(|c| c.stored_at.clone()).unwrap_or_default(),
            notes: l.notes.clone().unwrap_or_default(),
            show_status: true,
            bought_by: l.submitted_by.to_string(),
            users: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct VoyageFormView {
    pub bought_at: String,
    pub ship: String,
    pub notes: String,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct NewRunFormView {
    pub bought_at: String,
    /// Whose run it is (user id) -- the buyer, and the default seller.
    /// Defaults to the member logging it; pick a crewmate to keep their
    /// books for them.
    pub logged_by: String,
    pub users: Vec<User>,
    pub ship: String,
    pub status: String,
    pub stored_at: String,
    pub notes: String,
    pub loads: Vec<LoadFormView>,
    pub error: Option<String>,
    /// Render the panel open (after a 422, or when asked).
    pub open: bool,
}

#[derive(Clone, Default)]
pub struct ShareFormRow {
    pub to_user: String,
    pub paid_to: String,
    pub kind: String,
    pub amount: String,
    pub reason: String,
}

pub struct CloseFormView {
    pub sold_at: String,
    pub sell_terminal: String,
    pub price_mode: String,
    pub sell_scu: String,
    pub sell_lot: String,
    /// "balance" price mode only; the sale keeps the lot price.
    pub balance_before: String,
    pub balance_after: String,
    /// Which crate ids are ticked.
    pub selected: Vec<i64>,
    pub shares: Vec<ShareFormRow>,
    pub users: Vec<User>,
    /// Who sold it (user id): the load's buyer by default, any crew member
    /// when someone else is keeping the books.
    pub sold_by: String,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// totals, plan, ledger, board
// ---------------------------------------------------------------------------

pub struct TotalsView {
    pub committed_fmt: String,
    pub realised_fmt: String,
    pub open_voyages: usize,
    pub open_loads: usize,
    pub open_scu: u32,
}

impl TotalsView {
    pub fn new(t: &Totals) -> Self {
        Self {
            committed_fmt: fmt_auec(t.committed),
            realised_fmt: fmt_auec(t.realised),
            open_voyages: t.open_voyages,
            open_loads: t.open_loads,
            open_scu: t.open_scu,
        }
    }
}

pub struct PlanEntryView {
    pub n: usize,
    pub commodity_upper: String,
    pub boxes: Vec<(u32, u32)>,
    pub stop_no: u32,
    pub destination_label: String,
    pub depth: &'static str,
}

pub struct PlanCard {
    pub title: String,
    pub sub: String,
    pub suggest: bool,
    pub entries: Vec<PlanEntryView>,
    /// sc-cargo.space deep link, for hulled voyages.
    pub link: Option<String>,
    /// Plain footnote for cards without a command.
    pub foot: String,
    /// `Some` on a Suggested card: merge everything into `target`.
    pub merge: Option<MergeCmd>,
}

pub struct MergeCmd {
    pub label: String,
    pub target: i64,
    pub sources: Vec<i64>,
    pub why: String,
}

fn entries_of(loads: &[&Load]) -> (Vec<PlanEntryView>, String) {
    let order = stowage_order(loads);
    let mut scu = 0u32;
    let mut crates = 0usize;
    let mut stops: Vec<u32> = Vec::new();
    for e in &order {
        scu += e.sizes.iter().sum::<u32>();
        crates += e.sizes.len();
        if !stops.contains(&e.stop_no) {
            stops.push(e.stop_no);
        }
    }
    let entries = order
        .iter()
        .enumerate()
        .map(|(i, e)| PlanEntryView {
            n: i + 1,
            commodity_upper: e.commodity.to_uppercase(),
            boxes: super::model::mix_of(e.sizes.iter().copied()),
            stop_no: e.stop_no,
            destination_label: e
                .destination
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| "undecided".into()),
            depth: e.depth.label(),
        })
        .collect();
    let summary = format!(
        "{} stop{} · {scu} SCU · {crates} crate{}",
        stops.len(),
        if stops.len() == 1 { "" } else { "s" },
        if crates == 1 { "" } else { "s" }
    );
    (entries, summary)
}

/// The Stowage Plan: one card per hulled voyage, then the unassigned
/// clusters (a cluster of several is a Suggested card with a merge command).
pub fn plan_cards(voyages: &[Voyage]) -> Vec<PlanCard> {
    let live: Vec<&Voyage> = voyages.iter().filter(|v| v.open_loads().next().is_some()).collect();
    let mut cards = Vec::new();
    for v in live.iter().filter(|v| v.ship.is_some()) {
        let ship = v.ship.as_ref().expect("filtered");
        let loads: Vec<&Load> = v.open_loads().collect();
        let (entries, sub) = entries_of(&loads);
        let sizes: Vec<u32> = loads
            .iter()
            .flat_map(|l| l.crates.iter())
            .filter(|c| c.is_live())
            .map(|c| c.size_scu)
            .collect();
        cards.push(PlanCard {
            title: format!("Voyage {} · {}", v.id, v.hull_label().unwrap_or_default()),
            sub,
            suggest: false,
            entries,
            link: Some(sc_cargo_link(&ship.sc_cargo_slug, &sizes)),
            foot: String::new(),
            merge: None,
        });
    }
    for cluster in cluster_unassigned(&live) {
        let members: Vec<&Voyage> = cluster
            .iter()
            .filter_map(|id| live.iter().find(|v| v.id == *id).copied())
            .collect();
        let loads: Vec<&Load> = members.iter().flat_map(|v| v.open_loads()).collect();
        let (entries, sub) = entries_of(&loads);
        let nums = cluster.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" + ");
        if members.len() == 1 {
            cards.push(PlanCard {
                title: format!("Voyage {nums} · no hull yet"),
                sub,
                suggest: false,
                entries,
                link: None,
                foot: "Assign a hull to get the sc-cargo.space link.".into(),
                merge: None,
            });
            continue;
        }
        // destinations named by more than one member
        let mut shared: Vec<String> = Vec::new();
        for (i, a) in members.iter().enumerate() {
            for d in a.destinations() {
                let elsewhere = members
                    .iter()
                    .enumerate()
                    .any(|(j, b)| j != i && b.destinations().iter().any(|e| e.eq_ignore_ascii_case(&d)));
                if elsewhere && !shared.iter().any(|s| s.eq_ignore_ascii_case(&d)) {
                    shared.push(d);
                }
            }
        }
        let target = cluster[0];
        cards.push(PlanCard {
            title: format!("Suggested · Voyages {nums}"),
            sub,
            suggest: true,
            entries,
            link: None,
            foot: String::new(),
            merge: Some(MergeCmd {
                label: format!("Merge {nums} into Voyage {target}"),
                target,
                sources: cluster[1..].to_vec(),
                why: shared.join(", "),
            }),
        });
    }
    cards
}

/// Sold loads newest first, with their voyage number.
pub struct LedgerRow {
    pub load: LoadView,
    pub voyage_id: i64,
}

pub fn ledger_rows(voyages: &[Voyage]) -> Vec<LedgerRow> {
    let mut sold: Vec<(&Voyage, &Load)> = voyages
        .iter()
        .flat_map(|v| v.loads.iter().filter(|l| l.is_sold()).map(move |l| (v, l)))
        .collect();
    sold.sort_by(|a, b| b.1.sold_at.cmp(&a.1.sold_at).then(b.1.id.cmp(&a.1.id)));
    sold.into_iter()
        .map(|(v, l)| LedgerRow { load: LoadView::new(l, v.hull_label().as_deref()), voyage_id: v.id })
        .collect()
}

pub struct StandingView {
    pub rank: usize,
    pub user_id: i64,
    pub display_name: String,
    pub handle: String,
    pub profit_fmt: String,
    pub profit_neg: bool,
    pub runs: usize,
    pub loads: usize,
    pub scu_fmt: String,
    pub route_from: String,
    pub route_to: String,
    pub has_route: bool,
    pub pct: u32,
    pub dim: bool,
}

impl StandingView {
    pub fn new(rank: usize, s: &Standing) -> Self {
        Self {
            rank,
            user_id: s.user_id,
            display_name: s.display_name.clone(),
            handle: s.handle.clone(),
            profit_fmt: if s.loads_sold == 0 && s.board_credit == 0.0 { "0".into() } else { fmt_signed(s.board_credit) },
            profit_neg: s.board_credit < 0.0,
            runs: s.runs_closed,
            loads: s.loads_sold,
            scu_fmt: fmt_auec(s.scu_hauled as f64),
            route_from: s.best_route.as_ref().map(|r| short_terminal(&r.from)).unwrap_or_default(),
            route_to: s.best_route.as_ref().map(|r| short_terminal(&r.to)).unwrap_or_default(),
            has_route: s.best_route.is_some(),
            pct: s.pct_of_org.round() as u32,
            dim: s.loads_sold == 0 && s.board_credit == 0.0,
        }
    }
}

pub struct RecentShareView {
    pub sold_short: String,
    pub load: String,
    pub from_upper: String,
    pub to_upper: String,
    pub terms: String,
    pub paid_fmt: String,
}

pub struct BoardView {
    pub rows: Vec<StandingView>,
    pub sort: &'static str,
    pub org_realised_fmt: String,
    pub closed_runs: usize,
    pub scu_hauled_fmt: String,
    pub shares_paid_fmt: String,
    pub recent_shares: Vec<RecentShareView>,
}

pub fn board_view(users: &[User], voyages: &[Voyage], key: SortKey) -> BoardView {
    let rows = standings(users, voyages, key);
    let realised: f64 = voyages.iter().flat_map(|v| v.loads.iter()).filter_map(|l| l.profit()).sum();
    let closed_runs = voyages.iter().filter(|v| v.is_fully_sold()).count();
    let scu_hauled: u32 = voyages.iter().flat_map(|v| v.loads.iter()).map(|l| l.scu_sold()).sum();
    let shares_paid: f64 = voyages.iter().flat_map(|v| v.loads.iter()).map(|l| l.shares_paid()).sum();
    let mut recent: Vec<(&Load, &Share)> = voyages
        .iter()
        .flat_map(|v| v.loads.iter())
        .flat_map(|l| l.shares.iter().map(move |s| (l, s)))
        .collect();
    recent.sort_by(|a, b| b.0.sold_at.cmp(&a.0.sold_at).then(b.1.id.cmp(&a.1.id)));
    let recent_shares = recent
        .into_iter()
        .take(12)
        .map(|(l, s)| {
            let sv = ShareView::new(s);
            RecentShareView {
                sold_short: l.sold_at.as_deref().map(short_date).unwrap_or_default(),
                load: format!("{} · {} SCU", l.commodity.to_uppercase(), l.scu_sold()),
                from_upper: l.submitted_by_name.to_uppercase(),
                to_upper: if sv.forfeited {
                    format!("{} → {}", sv.to_name, sv.paid_to_name)
                } else {
                    sv.to_name.clone()
                },
                terms: if sv.reason.is_empty() { sv.terms.clone() } else { format!("{} · {}", sv.terms, sv.reason) },
                paid_fmt: sv.paid_fmt.clone(),
            }
        })
        .collect();
    BoardView {
        rows: rows.iter().enumerate().map(|(i, s)| StandingView::new(i + 1, s)).collect(),
        sort: key.as_str(),
        org_realised_fmt: fmt_auec(realised),
        closed_runs,
        scu_hauled_fmt: fmt_auec(scu_hauled as f64),
        shares_paid_fmt: fmt_auec(shares_paid),
        recent_shares,
    }
}

pub struct StatView {
    pub label: &'static str,
    pub value: String,
    pub unit: String,
    /// Prose value (route, commodity, date) rather than a big figure.
    pub txt: bool,
}

pub struct RecordView {
    pub win: bool,
    pub name_upper: String,
    pub handle_line: String,
    pub line: String,
    pub stats: Vec<StatView>,
}

const WIN_LINES: [&str; 3] = [
    "TOP OF THE BOARD, %s. THE ORG EATS TONIGHT.",
    "%s HOLDS THE LINE. FIRST BY PROFIT.",
    "%s · THE LEDGER BOWS. NOBODY ELSE COMES CLOSE.",
];
const LOSE_LINES: [&str; 3] = [
    "%s · NOT THIS WEEK. HAUL MORE, SELL HIGHER.",
    "%s · THE BOARD IS UNMOVED. FLY SAFE, EARN MORE.",
    "%s · THE LEDGER HAS SEEN BETTER. IT HAS ALSO SEEN WORSE.",
];

pub fn record_view(rec: &ServiceRecord, seed: u64) -> RecordView {
    let win = rec.rank == Some(1) && rec.loads_sold > 0;
    let name_upper = rec.user.display_name.to_uppercase();
    let pool: &[&str] = if win { &WIN_LINES } else { &LOSE_LINES };
    let line = pool[(seed % pool.len() as u64) as usize].replace("%s", &name_upper);
    let fig = |label: &'static str, value: String, unit: &str| StatView {
        label,
        value,
        unit: unit.into(),
        txt: false,
    };
    let txt = |label: &'static str, value: String| StatView { label, value, unit: String::new(), txt: true };
    let dash = || "—".to_string();
    let mut stats = vec![fig("Lifetime profit", fmt_signed(rec.board_credit), "aUEC")];
    if (rec.cash - rec.board_credit).abs() > 0.5 {
        stats.push(fig("Cash position", fmt_signed(rec.cash), "aUEC"));
    }
    stats.extend([
        fig("Runs closed", rec.runs_closed.to_string(), ""),
        fig("Loads sold", rec.loads_sold.to_string(), ""),
        fig("SCU hauled", fmt_auec(rec.scu_hauled as f64), ""),
        txt(
            "Best trade",
            rec.best_trade
                .as_ref()
                .map(|(c, p)| format!("{} · {}", c.to_uppercase(), fmt_signed(*p)))
                .unwrap_or_else(dash),
        ),
        txt(
            "Worst trade",
            rec.worst_trade
                .as_ref()
                .map(|(c, p)| format!("{} · {}", c.to_uppercase(), fmt_signed(*p)))
                .unwrap_or_else(dash),
        ),
        txt(
            "Favourite commodity",
            rec.favourite_commodity
                .as_ref()
                .map(|(c, n)| format!("{} · {n} load{}", c.to_uppercase(), if *n == 1 { "" } else { "s" }))
                .unwrap_or_else(dash),
        ),
        txt(
            "Favourite route",
            rec.favourite_route
                .as_ref()
                .map(|(f, t, _)| format!("{} → {}", short_terminal(f), short_terminal(t)))
                .unwrap_or_else(dash),
        ),
        fig("Crates lost", rec.crates_lost.to_string(), &format!("· {} SCU", rec.scu_lost)),
        fig("Shares paid out", fmt_auec(rec.shares_paid_out), "aUEC"),
        fig("Shares received", fmt_auec(rec.shares_received), "aUEC"),
        txt(
            "Ships flown",
            if rec.ships_flown.is_empty() { dash() } else { rec.ships_flown.join(" · ") },
        ),
        txt("First sale", rec.first_sale.clone().unwrap_or_else(dash)),
        txt("Latest sale", rec.latest_sale.clone().unwrap_or_else(dash)),
    ]);
    RecordView {
        win,
        name_upper,
        handle_line: format!(
            "{} · aboard since {} · {}",
            rec.user.handle,
            rec.user.created_at.get(..10).unwrap_or(&rec.user.created_at),
            match rec.rank {
                Some(r) if rec.loads_sold > 0 => format!("rank {r} by profit"),
                _ => "no record yet".to_string(),
            }
        ),
        line,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_prices_and_margins_format_like_the_mock() {
        assert_eq!(fmt_signed(748_800.0), "+748,800");
        assert_eq!(fmt_signed(-118_400.0), "−118,400");
        assert_eq!(fmt_signed(0.0), "+0");
        assert_eq!(fmt_price(8.9), "8.90");
        assert_eq!(fmt_price(27.4), "27.40");
        assert_eq!(fmt_price(3600.0), "3,600.00");
        assert_eq!(fmt_price(0.999), "1.00");
        assert_eq!(fmt_margin(0.171), "+17.1%");
        assert_eq!(fmt_margin(-1.0), "−100.0%");
        assert_eq!(short_date("2026-11-07"), "11-07");
        assert_eq!(short_terminal("Shubin Mining Facility SCD-1"), "Shubin SCD-1");
        assert_eq!(short_terminal("ArcCorp Mining Area 141"), "ArcCorp 141");
        assert_eq!(trim_num(25.0), "25");
        assert_eq!(trim_num(12.5), "12.5");
    }

    #[test]
    fn edit_form_shows_a_per_lot_load_to_the_cent() {
        use crate::stowage::model::fixtures::{load, sound};
        // 1,000 aUEC for 3 SCU: the derived per-SCU price is 333.333…
        let mut l = load(1, 1000.0 / 3.0, vec![sound(1, 1), sound(2, 1), sound(3, 1)]);
        l.price_mode = crate::stowage::model::PriceMode::Lot;
        l.buy_price_lot = Some(1000.0);
        let f = LoadFormView::from_load(&l);
        assert_eq!(f.price_scu, "333.33");
        assert_eq!(f.price_lot, "1000");
    }
}
