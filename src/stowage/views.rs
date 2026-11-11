//! Flat, template-friendly views of the model. Every figure is formatted
//! here so the Askama templates only do field access, `{% if %}` and
//! `{% for %}`; nothing is computed in a template.

use super::model::{
    cluster_unassigned, format_boxes, parse_boxes, sc_cargo_link, standings, stowage_order, Crate, CrewMember, CrewTerms,
    Leg, Load, LoadStatus, MemberLedger, PlanEntry, ServiceRecord, Share, SortKey, Standing, Totals, User, Voyage,
    Whereabouts,
};
use crate::templates::fmt_auec;

// ---------------------------------------------------------------------------
// formatting helpers
// ---------------------------------------------------------------------------

/// The SCU a typed crate mix adds up to, for the read-only SCU field of a
/// load form; empty while the boxes don't parse. Crates define the SCU, so
/// this is the only place the field's value comes from.
pub fn scu_from_boxes(raw: &str) -> String {
    match parse_boxes(raw) {
        Ok(b) => b.iter().map(|(n, s)| n * s).sum::<u32>().to_string(),
        Err(_) => String::new(),
    }
}

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
    /// A hauling contract: no cost, paid on delivery.
    pub is_haul: bool,
    /// A haul's promised payout, signed; empty when not given.
    pub reward_fmt: String,
    /// The "Cost / reward" column for a haul: the contract reward, or what
    /// the delivery paid when no reward was noted; empty when neither.
    pub haul_reward_fmt: String,
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
    /// Route stop it boards at (0 = the start).
    pub pickup_stop: u32,
    /// Route stop it comes off at, or `—` while undecided.
    pub drop_label: String,
    pub destination_label: String,
    pub where_now: String,
    pub cost_fmt: String,
    pub written_off_fmt: String,
    pub has_write_off: bool,
    /// Its voyage has other open loads, so `[DETACH]` means something.
    pub can_detach: bool,
    /// Some live crate isn't aboard yet, so the row offers `[LOADED]`.
    pub can_load: bool,
    pub by_upper: String,
    /// Who holds it (the seller, on a sold row), for a link to their ledger.
    pub by_id: i64,
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
    /// `v` is the load's voyage: its hull and route tell the where-now.
    pub fn new(l: &Load, v: &Voyage) -> Self {
        let status = l.status();
        let stored_at = l
            .crates
            .iter()
            .find_map(|c| c.stored_at.clone())
            .unwrap_or_default();
        Self {
            id: l.id,
            voyage_id: l.voyage_id,
            is_haul: l.is_haul(),
            reward_fmt: l.reward.map(fmt_signed).unwrap_or_default(),
            haul_reward_fmt: if l.is_haul() { l.reward.or_else(|| l.revenue()).map(fmt_signed).unwrap_or_default() } else { String::new() },
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
            pickup_stop: l.pickup_stop,
            drop_label: l.drop_stop.map(|d| d.to_string()).unwrap_or_else(|| "—".into()),
            destination_label: l
                .destination
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| "undecided".into()),
            where_now: v.where_now(l),
            cost_fmt: fmt_auec(l.cost()),
            written_off_fmt: fmt_auec(l.written_off()),
            has_write_off: l.scu_lost() > 0,
            can_detach: false,
            can_load: l.crates.iter().any(|c| c.is_live() && c.whereabouts != Whereabouts::Aboard),
            by_upper: l.submitted_by_name.to_uppercase(),
            by_id: l.submitted_by,
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
            // nothing was paid for a haul, so there's no margin on it
            margin_fmt: if l.is_haul() { "—".into() } else { l.margin().map(fmt_margin).unwrap_or_default() },
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

/// One stop on a voyage's route, for the header's route line.
pub struct RouteStopView {
    pub n: u32,
    pub name: String,
    pub short: String,
    /// The voyage is here.
    pub here: bool,
    /// Already left behind.
    pub past: bool,
}

pub struct VoyageView {
    pub id: i64,
    pub route: Vec<RouteStopView>,
    /// `[NEXT STOP ▸ …]`: the next stop's short name, when there is one.
    pub next_stop: Option<String>,
    pub label: String,
    pub date_short: String,
    pub ship_name: String,
    pub logged_by_upper: String,
    pub load_count: usize,
    pub stop_count: usize,
    pub notes: String,
    /// `LORD GIG · JT 25%`: who's aboard and who holds a share.
    pub crew_line: String,
    /// Open loads only (sold ones live on the Ledger).
    pub loads: Vec<LoadView>,
    pub extra: VoyageExtra,
}

impl VoyageView {
    pub fn new(v: &Voyage) -> Self {
        let open: Vec<&Load> = v.open_loads().collect();
        Self {
            id: v.id,
            route: (0..v.route.len() as u32)
                .map(|n| {
                    let name = v.stop_name(n);
                    RouteStopView { n, short: short_terminal(&name), name, here: n == v.at_stop, past: n < v.at_stop }
                })
                .collect(),
            next_stop: v.next_stop().map(|n| short_terminal(&v.stop_name(n))),
            label: format!("Voyage {}", v.id),
            date_short: short_date(&v.bought_at),
            ship_name: v.ship.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "—".into()),
            logged_by_upper: v.entered_by_name.to_uppercase(),
            load_count: open.len(),
            stop_count: v.stop_count(),
            notes: v.notes.clone().unwrap_or_default(),
            crew_line: crew_line(v),
            loads: open
                .iter()
                .map(|l| LoadView { can_detach: open.len() > 1, ..LoadView::new(l, v) })
                .collect(),
            extra: VoyageExtra::None,
        }
    }
}

/// The voyage's crew in one line, shares in short: `LORD GIG · JT 25%+1,000`.
pub fn crew_line(v: &Voyage) -> String {
    v.crew
        .iter()
        .map(|c| {
            let t = &c.terms;
            let mut s = c.name.to_uppercase();
            match (t.pct > 0.0, t.flat > 0.0) {
                (true, true) => s += &format!(" {}%+{}", trim_num(t.pct), fmt_auec(t.flat)),
                (true, false) => s += &format!(" {}%", trim_num(t.pct)),
                (false, true) => s += &format!(" {}", fmt_auec(t.flat)),
                (false, false) => {}
            }
            if let Some(p) = &c.paid_to_name {
                s += &format!(" → {}", p.to_uppercase());
            }
            s
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

// ---------------------------------------------------------------------------
// forms (also carry submitted values back on a 422)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct LoadFormView {
    /// `trade` (bought commodity) or `haul` (hauling contract).
    pub kind: String,
    /// A haul's contract reward.
    pub reward: String,
    pub commodity: String,
    pub boxes: String,
    pub scu: String,
    pub price_mode: String,
    pub price_scu: String,
    pub price_lot: String,
    /// "balance" price mode only; never stored -- the load keeps the lot price.
    pub balance_before: String,
    pub balance_after: String,
    /// Route stop keys it's picked up at and dropped at (`""` = undecided).
    pub pickup: String,
    pub drop: String,
    /// The route stops to pick from: `(key, label)`. Empty in the New Run
    /// template line, whose options the page builds from the route rows.
    pub stops: Vec<(String, String)>,
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
    /// A fresh line: picked up at the first stop offered, dropped at the last.
    pub fn blank(stops: Vec<(String, String)>) -> Self {
        Self {
            kind: "trade".into(),
            price_mode: "scu".into(),
            pickup: stops.first().map(|s| s.0.clone()).unwrap_or_default(),
            drop: if stops.len() > 1 { stops.last().map(|s| s.0.clone()).unwrap_or_default() } else { String::new() },
            stops,
            // bought cargo, or a contract just taken, still sits at the terminal
            status: "awaiting_pickup".into(),
            ..Default::default()
        }
    }

    /// The edit form of a load on voyage `v` (its route is what to pick from).
    pub fn from_load(l: &Load, v: &Voyage) -> Self {
        let status = match l.status() {
            LoadStatus::AwaitingPickup => "awaiting_pickup",
            LoadStatus::InTransit => "aboard",
            LoadStatus::Stored => "stored",
            _ => "",
        };
        Self {
            kind: l.kind.as_str().into(),
            reward: l.reward.map(|r| trim_num(to_cents(r))).unwrap_or_default(),
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
            pickup: l.pickup_stop.to_string(),
            drop: l.drop_stop.map(|d| d.to_string()).unwrap_or_default(),
            stops: route_options(v),
            status: status.into(),
            stored_at: l.crates.iter().find_map(|c| c.stored_at.clone()).unwrap_or_default(),
            notes: l.notes.clone().unwrap_or_default(),
            show_status: true,
            bought_by: l.bought_by.to_string(),
            users: Vec::new(),
            error: None,
        }
    }
}

/// A voyage's route as pick options keyed by stop number, for the Add Load
/// and Edit Load forms (whose route can't change under them).
pub fn route_options(v: &Voyage) -> Vec<(String, String)> {
    (0..v.route.len().max(1) as u32).map(|n| (n.to_string(), stop_option_label(n, &v.stop_name(n)))).collect()
}

/// `Start · ArcCorp 141` / `Stop 2 · Lorville TDD` (as static/ripae.js spells it).
pub fn stop_option_label(n: u32, name: &str) -> String {
    let name = if name.trim().is_empty() { "—".to_string() } else { short_terminal(name) };
    if n == 0 {
        format!("Start · {name}")
    } else {
        format!("Stop {n} · {name}")
    }
}

/// A route row in the New Run / Edit Voyage route editor. `key` names the
/// row whatever its position, so the loads' picks survive moving it.
#[derive(Clone, Default)]
pub struct RouteRow {
    pub key: String,
    pub terminal: String,
}

/// An open load's pickup / drop picks in the Edit Voyage route editor.
#[derive(Clone, Default)]
pub struct RouteAssign {
    pub load_id: i64,
    pub label: String,
    pub pickup: String,
    pub drop: String,
}

#[derive(Clone, Default)]
pub struct RouteFormView {
    /// Start first.
    pub stops: Vec<RouteRow>,
    /// Edit Voyage only: where the voyage is (a row key) and the open loads.
    pub at: String,
    pub show_at: bool,
    pub loads: Vec<RouteAssign>,
}

impl RouteFormView {
    /// `(key, label)` pick options for every row, in order.
    pub fn options(&self) -> Vec<(String, String)> {
        self.stops
            .iter()
            .enumerate()
            .map(|(i, r)| (r.key.clone(), stop_option_label(i as u32, &r.terminal)))
            .collect()
    }
}

#[derive(Clone, Default)]
pub struct VoyageFormView {
    pub bought_at: String,
    pub ship: String,
    pub notes: String,
    /// Who typed the voyage in (user id), correctable here; the crew list.
    pub entered_by: String,
    pub users: Vec<User>,
    /// The voyage's crew and the terms every sale on it splits by.
    pub crew: Vec<CrewFormRow>,
    /// Whose run it is (user id).
    pub owner: String,
    /// Also move the old owner's unsold loads to a new owner.
    pub move_loads: bool,
    /// The route editor: open voyages only (the ledger's overlay leaves it out).
    pub route: Option<RouteFormView>,
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
    pub route: RouteFormView,
    pub loads: Vec<LoadFormView>,
    pub error: Option<String>,
    /// Render the panel open (after a 422, or when asked).
    pub open: bool,
}

/// One crew member on the voyage form, as typed or as stored.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct CrewFormRow {
    pub user_id: String,
    /// Percent of each sale's profit; blank = none.
    pub pct: String,
    /// Flat fee per sale; blank = none.
    pub flat: String,
    pub paid_to: String,
    pub reason: String,
    /// The voyage names them (owner, buyer, seller, share holder), so they
    /// stay aboard: no [REMOVE].
    pub fixed: bool,
}

impl CrewFormRow {
    pub fn of(t: &CrewTerms, fixed: bool) -> Self {
        let num = |n: f64| if n > 0.0 { trim_num(n) } else { String::new() };
        CrewFormRow {
            user_id: t.user_id.to_string(),
            pct: num(t.pct),
            flat: num(t.flat),
            paid_to: t.paid_to.map(|p| p.to_string()).unwrap_or_default(),
            reason: t.reason.clone().unwrap_or_default(),
            fixed,
        }
    }
}

/// A split line shown (read-only) on a sale form: who, on what terms, and
/// the figures the live preview pays out from.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct SplitLineView {
    pub user_id: i64,
    pub name_upper: String,
    pub terms: String,
    /// Plain numbers for the preview script.
    pub pct: String,
    pub flat: String,
    pub forfeit_upper: String,
    pub reason: String,
}

/// `25% of profit + 1,000 flat`, `flat 1,000`, or `no share`.
pub fn terms_label(pct: f64, flat: f64) -> String {
    match (pct > 0.0, flat > 0.0) {
        (true, true) => format!("{}% of profit + {} flat", trim_num(pct), fmt_auec(flat)),
        (true, false) => format!("{}% of profit", trim_num(pct)),
        (false, true) => format!("{} flat", fmt_auec(flat)),
        (false, false) => "no share".into(),
    }
}

impl SplitLineView {
    /// A crew member's terms on the voyage.
    pub fn crew(c: &CrewMember) -> Self {
        SplitLineView {
            user_id: c.terms.user_id,
            name_upper: c.name.to_uppercase(),
            terms: terms_label(c.terms.pct, c.terms.flat),
            pct: trim_num(c.terms.pct),
            flat: trim_num(c.terms.flat),
            forfeit_upper: c.paid_to_name.as_deref().map(str::to_uppercase).unwrap_or_default(),
            reason: c.terms.reason.clone().unwrap_or_default(),
        }
    }

    /// A share already settled on a sale, as its terms.
    pub fn recorded(s: &Share) -> Self {
        let (pct, flat) = match s.kind {
            super::model::ShareKind::Pct => (s.amount, 0.0),
            super::model::ShareKind::Flat => (0.0, s.amount),
        };
        SplitLineView {
            user_id: s.to_user,
            name_upper: s.to_user_name.to_uppercase(),
            terms: terms_label(pct, flat),
            pct: trim_num(pct),
            flat: trim_num(flat),
            forfeit_upper: if s.paid_to != s.to_user { s.paid_to_name.to_uppercase() } else { String::new() },
            reason: s.reason.clone().unwrap_or_default(),
        }
    }
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
    /// Write off (mark lost) the glitched crates left unsold, so they don't
    /// hold the load open. On by default.
    pub write_off_glitched: bool,
    /// The voyage crew's terms: what a sale settles (less the seller's own).
    pub crew: Vec<SplitLineView>,
    /// Sale editor only: the shares as recorded on the sale, and whether to
    /// re-split it by the crew's terms as they stand instead.
    pub recorded: Vec<SplitLineView>,
    pub resplit: bool,
    /// The sale editor rather than the close form.
    pub editing: bool,
    pub voyage_id: i64,
    pub users: Vec<User>,
    /// Who sold it (user id): the load's buyer by default, any crew member
    /// when someone else is keeping the books.
    pub sold_by: String,
    /// Who bought it (user id); only the sale editor offers it.
    pub bought_by: String,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// totals, plan, ledger, board
// ---------------------------------------------------------------------------

pub struct TotalsView {
    pub committed_fmt: String,
    pub realised_fmt: String,
    /// Realised is a net loss: shown in red, the one exception to the
    /// green-on-black rule on the totals tiles.
    pub realised_neg: bool,
    pub open_voyages: usize,
    pub open_loads: usize,
    pub open_scu: u32,
}

impl TotalsView {
    pub fn new(t: &Totals) -> Self {
        Self {
            committed_fmt: fmt_auec(t.committed),
            realised_fmt: fmt_auec(t.realised),
            realised_neg: t.realised.round() < 0.0,
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
    /// `off at stop 2 · Lorville TDD` / `bound for undecided`.
    pub route_label: String,
    pub depth: &'static str,
    /// Came aboard at the stop this leg starts from.
    pub boarded: bool,
    /// Under cargo that comes off later: it has to be dug out.
    pub buried: bool,
}

/// One leg of a voyage's plan: what's aboard from one stop to the next.
pub struct LegView {
    /// `Leg 1 · Shubin SCD-1 → Area18 TDD`.
    pub title: String,
    /// The leg the voyage is on (it last reached this leg's start).
    pub now: bool,
    pub past: bool,
    pub entries: Vec<PlanEntryView>,
    /// `Off at Area18 TDD: LARANITE` -- what came off before this leg.
    pub unloaded: String,
    /// Restows this leg forces, spelled out.
    pub warnings: Vec<String>,
    /// sc-cargo.space link for this leg's crates, for hulled voyages.
    pub link: Option<String>,
}

pub struct PlanCard {
    pub title: String,
    pub sub: String,
    pub suggest: bool,
    /// The plan leg by leg along the voyage's route.
    pub legs: Vec<LegView>,
    /// Suggested cards only: the loads of every voyage it would merge, in
    /// drop order (their routes differ until merged).
    pub entries: Vec<PlanEntryView>,
    /// sc-cargo.space deep link for the leg the voyage is on, when hulled.
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

fn destination_label(e: &PlanEntry) -> String {
    e.destination.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| "undecided".into())
}

fn entry_view(n: usize, e: &PlanEntry, boarded: bool, buried: bool) -> PlanEntryView {
    PlanEntryView {
        n,
        commodity_upper: e.commodity.to_uppercase(),
        boxes: super::model::mix_of(e.sizes.iter().copied()),
        route_label: match e.drop_stop {
            Some(d) => format!("off at stop {d} · {}", destination_label(e)),
            None => "bound for undecided".into(),
        },
        depth: e.depth.label(),
        boarded,
        buried,
    }
}

/// `"2 stops · 88 SCU · 5 crates"` over the open loads.
fn plan_summary(stops: usize, loads: &[&Load]) -> String {
    let live = loads.iter().flat_map(|l| l.crates.iter()).filter(|c| c.is_live());
    let (scu, crates) = live.fold((0u32, 0usize), |(s, n), c| (s + c.size_scu, n + 1));
    format!(
        "{stops} stop{} · {scu} SCU · {crates} crate{}",
        if stops == 1 { "" } else { "s" },
        if crates == 1 { "" } else { "s" }
    )
}

fn leg_view(v: &Voyage, leg: &Leg, slug: Option<&str>) -> LegView {
    let name = |id: i64| {
        v.loads.iter().find(|l| l.id == id).map(|l| l.commodity.to_uppercase()).unwrap_or_else(|| format!("load {id}"))
    };
    let to = match leg.to {
        Some(t) => short_terminal(&v.stop_name(t)),
        None => "onward (undecided)".into(),
    };
    let from = short_terminal(&v.stop_name(leg.from));
    let warnings = leg
        .conflicts
        .iter()
        .map(|c| {
            let off = v.loads.iter().find(|l| l.id == c.under).and_then(|l| l.drop_stop);
            format!(
                "{} boards on top of {}, which comes off first{} -- restow {} on top here, or shift {} aside there.",
                name(c.over),
                name(c.under),
                off.map(|d| format!(" at {}", short_terminal(&v.stop_name(d)))).unwrap_or_default(),
                name(c.under),
                name(c.over)
            )
        })
        .collect();
    let sizes: Vec<u32> = leg.aboard.iter().flat_map(|e| e.entry.sizes.iter().copied()).collect();
    LegView {
        title: format!("Leg {} · {from} → {to}", leg.n),
        now: leg.from == v.at_stop,
        past: leg.from < v.at_stop,
        entries: leg
            .aboard
            .iter()
            .enumerate()
            .map(|(i, e)| entry_view(i + 1, &e.entry, e.boarded, e.buried))
            .collect(),
        unloaded: if leg.unloaded.is_empty() {
            String::new()
        } else {
            format!("Off at {from}: {}", leg.unloaded.iter().map(|&id| name(id)).collect::<Vec<_>>().join(", "))
        },
        warnings,
        link: slug.filter(|_| !sizes.is_empty()).map(|s| sc_cargo_link(s, &sizes)),
    }
}

/// A voyage's legs; the card's own link is the leg the voyage is on (or
/// the first leg with cargo, before it sets off).
fn legs_of(v: &Voyage) -> (Vec<LegView>, Option<String>) {
    let slug = v.ship.as_ref().map(|s| s.sc_cargo_slug.as_str());
    let legs: Vec<LegView> = v.legs().iter().map(|l| leg_view(v, l, slug)).collect();
    let link = legs
        .iter()
        .find(|l| l.now && l.link.is_some())
        .or_else(|| legs.iter().find(|l| l.link.is_some()))
        .and_then(|l| l.link.clone());
    (legs, link)
}

/// The Stowage Plan: one card per hulled voyage, then the unassigned
/// clusters (a cluster of several is a Suggested card with a merge command).
pub fn plan_cards(voyages: &[Voyage]) -> Vec<PlanCard> {
    let live: Vec<&Voyage> = voyages.iter().filter(|v| v.open_loads().next().is_some()).collect();
    let mut cards = Vec::new();
    for v in live.iter().filter(|v| v.ship.is_some()) {
        let loads: Vec<&Load> = v.open_loads().collect();
        let (legs, link) = legs_of(v);
        cards.push(PlanCard {
            title: format!("Voyage {} · {}", v.id, v.hull_label().unwrap_or_default()),
            sub: plan_summary(v.stop_count(), &loads),
            suggest: false,
            legs,
            entries: Vec::new(),
            link,
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
        let nums = cluster.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" + ");
        if members.len() == 1 {
            let v = members[0];
            let (legs, _) = legs_of(v);
            cards.push(PlanCard {
                title: format!("Voyage {nums} · no hull yet"),
                sub: plan_summary(v.stop_count(), &loads),
                suggest: false,
                legs,
                entries: Vec::new(),
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
        let mut dests: Vec<String> = Vec::new();
        for v in &members {
            for d in v.destinations() {
                if !dests.iter().any(|e| e.eq_ignore_ascii_case(&d)) {
                    dests.push(d);
                }
            }
        }
        let target = cluster[0];
        cards.push(PlanCard {
            title: format!("Suggested · Voyages {nums}"),
            sub: plan_summary(dests.len(), &loads),
            suggest: true,
            legs: Vec::new(),
            entries: stowage_order(&loads).iter().enumerate().map(|(i, e)| entry_view(i + 1, e, false, false)).collect(),
            link: None,
            foot: "Merged, they share one route and the plan runs leg by leg.".into(),
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
        .flat_map(|v| v.loads.iter().filter(|l| l.was_sold()).map(move |l| (v, l)))
        .collect();
    sold.sort_by(|a, b| b.1.sold_at.cmp(&a.1.sold_at).then(b.1.id.cmp(&a.1.id)));
    sold.into_iter()
        .map(|(v, l)| LedgerRow { load: LoadView::new(l, v), voyage_id: v.id })
        .collect()
}

pub struct StandingView {
    pub rank: usize,
    pub user_id: i64,
    pub display_name: String,
    pub handle: String,
    pub profit_fmt: String,
    pub profit_neg: bool,
    pub cash_fmt: String,
    pub cash_neg: bool,
    /// `−570,000 forfeited` when the member gave shares away, else empty.
    pub forfeited_fmt: String,
    /// `+3,200 from forfeits` when someone forfeited to them, else empty.
    pub forfeits_in_fmt: String,
    pub runs: usize,
    pub loads: usize,
    pub scu_fmt: String,
    pub route_from: String,
    pub route_to: String,
    pub has_route: bool,
    /// Bar width, 0–100.
    pub pct: u32,
    /// `100%`, `37%`, `<1%`, `0%`.
    pub pct_label: String,
    pub dim: bool,
}

/// Whole-percent label for a share bar; a sliver that rounds to 0 reads `<1%`.
pub fn pct_label(pct: f64) -> String {
    if pct > 0.0 && pct < 0.5 {
        "<1%".into()
    } else {
        format!("{}%", pct.round().clamp(0.0, 100.0) as u32)
    }
}

/// `0` for an idle member, else signed.
fn fmt_figure(amount: f64, idle: bool) -> String {
    if idle || amount.round() == 0.0 { "0".into() } else { fmt_signed(amount) }
}

impl StandingView {
    pub fn new(rank: usize, s: &Standing) -> Self {
        let idle = s.is_idle();
        Self {
            rank,
            user_id: s.user_id,
            display_name: s.display_name.clone(),
            handle: s.handle.clone(),
            profit_fmt: fmt_figure(s.board_credit, idle),
            profit_neg: s.board_credit.round() < 0.0,
            cash_fmt: fmt_figure(s.cash, idle),
            cash_neg: s.cash.round() < 0.0,
            forfeited_fmt: if s.forfeited.round() > 0.0 { format!("−{} forfeited", fmt_auec(s.forfeited)) } else { String::new() },
            forfeits_in_fmt: if s.forfeits_received.round() > 0.0 { format!("+{} from forfeits", fmt_auec(s.forfeits_received)) } else { String::new() },
            runs: s.runs_closed,
            loads: s.loads_sold,
            scu_fmt: fmt_auec(s.scu_hauled as f64),
            route_from: s.best_route.as_ref().map(|r| short_terminal(&r.from)).unwrap_or_default(),
            route_to: s.best_route.as_ref().map(|r| short_terminal(&r.to)).unwrap_or_default(),
            has_route: s.best_route.is_some(),
            pct: s.share_of_org.round().clamp(0.0, 100.0) as u32,
            pct_label: pct_label(s.share_of_org),
            dim: idle,
        }
    }
}

/// One `[BY …]` sort command and its column header.
pub struct SortLink {
    pub key: &'static str,
    pub label: &'static str,
    pub on: bool,
}

/// What the share bar measures under each sort.
pub fn share_heading(key: SortKey) -> &'static str {
    match key {
        SortKey::Profit => "Share of org profit",
        SortKey::Collected => "Share of org collected",
        SortKey::Runs => "Share of org runs",
        SortKey::Scu => "Share of org SCU hauled",
    }
}

fn share_hint(key: SortKey) -> &'static str {
    match key {
        SortKey::Profit => "Each member's profit (credit) as a share of the org's; adds up to 100%.",
        SortKey::Collected => "Each member's collected aUEC as a share of the org's; adds up to 100%.",
        SortKey::Runs => "Closed runs the member flew, out of the org's. A shared run counts for everyone aboard, so these don't add up to 100%.",
        SortKey::Scu => "SCU sold on loads the member sold or held a share of, out of the org's. Shared loads count for everyone on them, so these don't add up to 100%.",
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
    pub sorts: Vec<SortLink>,
    pub share_head: &'static str,
    pub share_hint: &'static str,
    pub org_realised_fmt: String,
    pub closed_runs: usize,
    pub scu_hauled_fmt: String,
    pub shares_paid_fmt: String,
    pub recent_shares: Vec<RecentShareView>,
}

pub fn board_view(users: &[User], voyages: &[Voyage], key: SortKey) -> BoardView {
    let rows = standings(users, voyages, key);
    let realised = Totals::from_voyages(voyages).realised;
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
        sorts: SortKey::ALL
            .iter()
            .map(|k| SortLink {
                key: k.as_str(),
                label: match k {
                    SortKey::Profit => "By profit",
                    SortKey::Collected => "By collected",
                    SortKey::Runs => "By runs",
                    SortKey::Scu => "By SCU",
                },
                on: *k == key,
            })
            .collect(),
        share_head: share_heading(key),
        share_hint: share_hint(key),
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
    pub user_id: i64,
    pub win: bool,
    pub name_upper: String,
    pub handle_line: String,
    pub avatar_url: Option<String>,
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
    let mut stats = vec![
        fig("Lifetime profit", fmt_signed(rec.board_credit), "aUEC"),
        fig("Collected", fmt_signed(rec.cash), "aUEC"),
    ];
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
        fig(
            "Crates lost",
            rec.crates_lost.to_string(),
            &if rec.written_off > 0.0 {
                format!("· {} SCU · −{} aUEC", rec.scu_lost, fmt_auec(rec.written_off))
            } else {
                format!("· {} SCU", rec.scu_lost)
            },
        ),
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
        user_id: rec.user.id,
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
        avatar_url: rec.user.avatar_url.clone(),
        line,
        stats,
    }
}

// ---------------------------------------------------------------------------
// a member's ledger
// ---------------------------------------------------------------------------

/// `0` for nothing, else signed: a ledger cell that's empty reads as such.
fn fmt_cell(amount: f64) -> String {
    if amount.round() == 0.0 {
        "0".into()
    } else {
        fmt_signed(amount)
    }
}

pub struct LedgerLoadView {
    pub commodity_upper: String,
    pub is_haul: bool,
    pub route: String,
    /// `sold 11-07` or `open`.
    pub when: String,
    pub scu: u32,
    pub seller_upper: String,
    pub seller_id: i64,
    pub by_member: bool,
    pub sold: bool,
    pub profit_fmt: String,
    pub profit_neg: bool,
    pub kept_fmt: String,
    pub share_fmt: String,
    pub received_fmt: String,
    pub losses_fmt: String,
    pub has_losses: bool,
}

pub struct LedgerVoyageView {
    pub voyage_id: i64,
    pub date_short: String,
    pub ship: String,
    pub roles: String,
    /// `25% of profit · forfeited to LORD GIG`, or empty.
    pub terms: String,
    pub open: bool,
    pub loads_sold: usize,
    pub scu_sold: u32,
    pub kept_fmt: String,
    pub share_fmt: String,
    pub collected_fmt: String,
    pub losses_fmt: String,
    pub credit_fmt: String,
    pub credit_neg: bool,
    pub running_credit_fmt: String,
    pub running_cash_fmt: String,
    pub loads: Vec<LedgerLoadView>,
}

pub struct MemberLedgerView {
    pub user_id: i64,
    pub name_upper: String,
    pub handle: String,
    pub voyages: Vec<LedgerVoyageView>,
    pub voyage_count: usize,
    pub loads_sold: usize,
    pub scu_sold_fmt: String,
    pub kept_fmt: String,
    pub share_fmt: String,
    pub collected_fmt: String,
    pub losses_fmt: String,
    pub credit_fmt: String,
    pub credit_neg: bool,
    pub cash_fmt: String,
    pub cash_neg: bool,
}

pub fn member_ledger_view(user: &User, users: &[User], m: &MemberLedger) -> MemberLedgerView {
    let name_of = |id: i64| users.iter().find(|u| u.id == id).map(|u| u.display_name.to_uppercase()).unwrap_or_default();
    let voyages = m
        .voyages
        .iter()
        .map(|v| LedgerVoyageView {
            voyage_id: v.voyage_id,
            date_short: short_date(&v.bought_at),
            ship: v.ship.clone().unwrap_or_else(|| "—".into()),
            roles: v.roles.iter().map(|r| r.label()).collect::<Vec<_>>().join(" · "),
            terms: v
                .terms
                .as_ref()
                .map(|t| match t.paid_to {
                    Some(p) => format!("{} · forfeited to {}", terms_label(t.pct, t.flat), name_of(p)),
                    None => terms_label(t.pct, t.flat),
                })
                .unwrap_or_default(),
            open: v.open,
            loads_sold: v.loads_sold,
            scu_sold: v.scu_sold,
            kept_fmt: fmt_cell(v.kept),
            share_fmt: fmt_cell(v.share_earned),
            collected_fmt: fmt_cell(v.collected),
            losses_fmt: fmt_cell(-v.losses),
            credit_fmt: fmt_cell(v.credit),
            credit_neg: v.credit.round() < 0.0,
            running_credit_fmt: fmt_signed(v.running_credit),
            running_cash_fmt: fmt_signed(v.running_cash),
            loads: v
                .loads
                .iter()
                .map(|l| LedgerLoadView {
                    commodity_upper: l.commodity.to_uppercase(),
                    is_haul: l.kind == super::model::LoadKind::Haul,
                    route: match &l.sell_terminal {
                        Some(to) if !l.buy_terminal.is_empty() => format!("{} → {}", short_terminal(&l.buy_terminal), short_terminal(to)),
                        Some(to) => format!("→ {}", short_terminal(to)),
                        None => short_terminal(&l.buy_terminal),
                    },
                    when: match (&l.sold_at, l.sold) {
                        (Some(d), true) => format!("sold {}", short_date(d)),
                        _ => "open".into(),
                    },
                    scu: l.scu,
                    seller_upper: l.seller_name.to_uppercase(),
                    seller_id: l.seller_id,
                    by_member: l.by_member,
                    sold: l.sold,
                    profit_fmt: l.profit.map(fmt_signed).unwrap_or_else(|| "—".into()),
                    profit_neg: l.profit.is_some_and(|p| p.round() < 0.0),
                    kept_fmt: fmt_cell(l.kept),
                    share_fmt: fmt_cell(l.share_earned),
                    received_fmt: fmt_cell(l.collected),
                    losses_fmt: fmt_cell(-l.losses),
                    has_losses: l.losses > 0.0,
                })
                .collect(),
        })
        .collect();
    let t = &m.totals;
    MemberLedgerView {
        user_id: user.id,
        name_upper: user.display_name.to_uppercase(),
        handle: user.handle.clone(),
        voyages,
        voyage_count: t.voyages,
        loads_sold: t.loads_sold,
        scu_sold_fmt: fmt_auec(t.scu_sold as f64),
        kept_fmt: fmt_cell(t.kept),
        share_fmt: fmt_cell(t.share_earned),
        collected_fmt: fmt_cell(t.collected),
        losses_fmt: fmt_cell(-t.losses),
        // the board's own rule: a member with nothing on it reads a bare 0
        credit_fmt: if t.credit == 0.0 && t.loads_sold == 0 && t.share_earned == 0.0 { "0".into() } else { fmt_signed(t.credit) },
        credit_neg: t.credit.round() < 0.0,
        cash_fmt: fmt_signed(t.cash),
        cash_neg: t.cash.round() < 0.0,
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
    fn share_labels_round_and_never_pass_100() {
        assert_eq!(pct_label(100.0), "100%");
        assert_eq!(pct_label(99.6), "100%");
        assert_eq!(pct_label(37.4), "37%");
        assert_eq!(pct_label(0.3), "<1%");
        assert_eq!(pct_label(0.0), "0%");
        assert_eq!(share_heading(SortKey::Runs), "Share of org runs");
        assert_eq!(share_heading(SortKey::Scu), "Share of org SCU hauled");
    }

    #[test]
    fn the_board_shows_what_a_forfeit_cost_and_who_got_it() {
        use crate::stowage::model::fixtures::{sold_load, users, voyage, GIG, JT};
        use crate::stowage::model::ShareKind;
        let vs = vec![voyage(1, JT, None, vec![sold_load(1, JT, 10.0, 30.0, &[32], vec![(GIG, Some(JT), ShareKind::Pct, 100.0)])])];
        let b = board_view(&users(), &vs, SortKey::Profit);
        assert_eq!(b.share_head, "Share of org profit");
        assert_eq!(b.sorts.iter().filter(|s| s.on).map(|s| s.key).collect::<Vec<_>>(), vec!["profit"]);
        let gig = b.rows.iter().find(|r| r.user_id == GIG).unwrap();
        assert_eq!((gig.rank, gig.profit_fmt.as_str(), gig.cash_fmt.as_str()), (1, "+640", "0"));
        assert_eq!((gig.forfeited_fmt.as_str(), gig.pct_label.as_str(), gig.pct), ("−640 forfeited", "100%", 100));
        let jt = b.rows.iter().find(|r| r.user_id == JT).unwrap();
        assert_eq!((jt.profit_fmt.as_str(), jt.cash_fmt.as_str(), jt.forfeits_in_fmt.as_str()), ("0", "+960", "+640 from forfeits"));
        assert!(!jt.dim, "a member who sold isn't idle even at 0 profit");
        let by_runs = board_view(&users(), &vs, SortKey::Runs);
        assert_eq!(by_runs.share_head, "Share of org runs");
        assert!(by_runs.rows.iter().filter(|r| r.user_id != 3).all(|r| r.pct_label == "100%"));
    }

    #[test]
    fn edit_form_shows_a_per_lot_load_to_the_cent() {
        use crate::stowage::model::fixtures::{load, sound, voyage, GIG};
        // 1,000 aUEC for 3 SCU: the derived per-SCU price is 333.333…
        let mut l = load(1, 1000.0 / 3.0, vec![sound(1, 1), sound(2, 1), sound(3, 1)]);
        l.price_mode = crate::stowage::model::PriceMode::Lot;
        l.buy_price_lot = Some(1000.0);
        let f = LoadFormView::from_load(&l, &voyage(1, GIG, None, vec![]));
        assert_eq!(f.price_scu, "333.33");
        assert_eq!(f.price_lot, "1000");
    }

    #[test]
    fn ledger_page_totals_read_the_same_as_the_leaderboard_row() {
        use crate::stowage::model::fixtures::{cut, load, sold_load, sound, users, voyage, with_crew, GIG, JT, THIRD};
        use crate::stowage::model::{member_ledger, Condition, ShareKind, Whereabouts};
        let mut open = load(3, 50.0, vec![sound(31, 8), crate::stowage::model::fixtures::crate_(32, 8, Condition::Lost, Whereabouts::Aboard)]);
        open.submitted_by = JT;
        let vs = vec![
            with_crew(
                voyage(1, GIG, None, vec![sold_load(1, GIG, 10.0, 30.0, &[32, 32], vec![(JT, Some(THIRD), ShareKind::Pct, 25.0)])]),
                vec![cut(JT, 25.0, 0.0, Some(THIRD))],
            ),
            voyage(2, JT, None, vec![sold_load(2, JT, 10.0, 4.0, &[16], vec![(GIG, None, ShareKind::Flat, 99.0)]), open]),
        ];
        let us = users();
        let board = board_view(&us, &vs, SortKey::Profit);
        for u in &us {
            let page = member_ledger_view(u, &us, &member_ledger(u.id, &vs));
            let row = board.rows.iter().find(|r| r.user_id == u.id).unwrap();
            assert_eq!(page.credit_fmt, row.profit_fmt, "{}'s ledger total vs their board row", u.display_name);
            assert_eq!(page.cash_fmt, fmt_signed(crate::stowage::model::cash(u.id, &vs)));
            if let Some(top) = page.voyages.first() {
                assert_eq!(top.running_credit_fmt, fmt_signed(crate::stowage::model::board_credit(u.id, &vs)), "the newest voyage's running total is the total");
            }
        }
        let jt = member_ledger_view(&us[1], &us, &member_ledger(JT, &vs));
        assert_eq!(jt.voyages.iter().map(|v| v.voyage_id).collect::<Vec<_>>(), vec![2, 1]);
        assert_eq!(jt.voyages[1].terms, "25% of profit · forfeited to CREW-03");
        assert_eq!(jt.voyages[1].collected_fmt, "0", "JT's share went to Crew-03");
        assert_eq!(jt.voyages[0].roles, "logged · sold");
        assert!(jt.voyages[0].open);
    }

    #[test]
    fn the_scu_field_is_the_boxes_total() {
        assert_eq!(scu_from_boxes("2×32, 4x8"), "96");
        assert_eq!(scu_from_boxes("1x32 2x32"), "96");
        assert_eq!(scu_from_boxes(""), "");
        assert_eq!(scu_from_boxes("2x12"), "");
    }

    #[test]
    fn a_haul_shows_its_reward_in_the_cost_column() {
        use crate::stowage::model::fixtures::{load, sold_load, sound, voyage};
        use crate::stowage::model::LoadKind;
        let v = voyage(1, 1, None, vec![]);
        let trade = LoadView::new(&load(1, 10.0, vec![sound(1, 32)]), &v);
        assert_eq!(trade.haul_reward_fmt, "");
        let mut h = load(2, 0.0, vec![sound(2, 32)]);
        h.kind = LoadKind::Haul;
        h.reward = Some(45_000.0);
        assert_eq!(LoadView::new(&h, &v).haul_reward_fmt, "+45,000");
        h.reward = None;
        assert_eq!(LoadView::new(&h, &v).haul_reward_fmt, "");
        // a delivered haul with no reward noted shows what the delivery paid
        let mut d = sold_load(3, 1, 0.0, 1000.0, &[8], vec![]);
        d.kind = LoadKind::Haul;
        assert_eq!(LoadView::new(&d, &v).haul_reward_fmt, "+8,000");
    }

    #[test]
    fn a_new_load_waits_at_its_pickup_until_loaded() {
        use crate::stowage::model::fixtures::{load, ship, sound, voyage};
        use crate::stowage::model::{Condition, Crate};
        assert_eq!(LoadFormView::blank(vec![("0".into(), "Levski".into())]).status, "awaiting_pickup");
        let waiting = |c: Crate| Crate { whereabouts: Whereabouts::AwaitingPickup, ..c };
        let mut l = load(1, 10.0, vec![waiting(sound(1, 8)), waiting(sound(2, 8))]);
        let hull_c = voyage(1, 1, Some(ship("Hull-C", "hull-c")), vec![]);
        let bare = voyage(1, 1, None, vec![]);
        let v = LoadView::new(&l, &hull_c);
        assert!(v.can_load);
        assert_eq!(v.where_now, "awaiting pickup at ArcCorp Mining Area 141");
        // where-now is derived from the buy / pickup terminal, so it follows an edit
        l.buy_terminal = "Levski".into();
        assert_eq!(LoadView::new(&l, &bare).where_now, "awaiting pickup at Levski");
        // all aboard (a lost crate doesn't count): nothing left to load
        l.crates[0].whereabouts = Whereabouts::Aboard;
        l.crates[1].condition = Condition::Lost;
        assert!(!LoadView::new(&l, &bare).can_load);
    }

    #[test]
    fn detach_is_offered_only_beside_another_open_load() {
        use crate::stowage::model::fixtures::{load, sold_load, sound, voyage};
        // a sold sibling doesn't count: Open Cargo shows the load alone
        let alone = VoyageView::new(&voyage(1, 1, None, vec![load(1, 10.0, vec![sound(1, 32)]), sold_load(2, 1, 10.0, 12.0, &[32], vec![])]));
        assert!(alone.loads.iter().all(|l| !l.can_detach));
        let pair = VoyageView::new(&voyage(2, 1, None, vec![load(3, 10.0, vec![sound(3, 32)]), load(4, 10.0, vec![sound(4, 32)])]));
        assert_eq!(pair.loads.len(), 2);
        assert!(pair.loads.iter().all(|l| l.can_detach));
    }
}
