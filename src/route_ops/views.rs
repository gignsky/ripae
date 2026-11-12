//! View model for the Route Ops card (`RIPAE-0.3-ROUTE-OPS.md`): a
//! phase-aware re-presentation of the Stowage model. Nothing here is
//! stored -- phase, the hold gauge and the two faces (Orders / Tracker)
//! are all derived at render from the same `Voyage` the Ledger already
//! loads. The deepest-first stow order is `Voyage::legs()`
//! (`stowage_legs`), reused rather than re-derived, per the spec.

use crate::stowage::model::{expand_boxes, format_boxes, mix_of, Depth, Leg, LegEntry, Load, LoadStatus, Voyage, Whereabouts};
use crate::stowage::views::{short_terminal, LoadView};

/// Undecided sorts last.
fn off_key(drop: Option<u32>) -> u32 {
    drop.unwrap_or(u32::MAX)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Port,
    Underway,
    Arrived,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Phase::Port => "PORT",
            Phase::Underway => "UNDERWAY",
            Phase::Arrived => "ARRIVED",
        }
    }
    pub fn css(&self) -> &'static str {
        match self {
            Phase::Port => "port",
            Phase::Underway => "underway",
            Phase::Arrived => "arrived",
        }
    }
    /// The face it brings up automatically.
    pub fn face(&self) -> &'static str {
        match self {
            Phase::Underway => "track",
            Phase::Port | Phase::Arrived => "stow",
        }
    }
    pub fn face_label(&self) -> &'static str {
        if self.face() == "stow" {
            "Orders"
        } else {
            "Tracker"
        }
    }
    pub fn other_face_label(&self) -> &'static str {
        if self.face() == "stow" {
            "Tracker"
        } else {
            "Orders"
        }
    }
}

/// Loads at the current stop with cargo still to drop or load there
/// (`RIPAE-0.3-ROUTE-OPS.md` §2).
fn pending_pickups(v: &Voyage, at: u32) -> bool {
    v.open_loads().any(|l| l.pickup_stop == at && l.status() != LoadStatus::InTransit)
}
fn pending_drops(v: &Voyage, at: u32) -> bool {
    v.open_loads().any(|l| l.drop_stop == Some(at))
}

pub fn phase_of(v: &Voyage) -> Phase {
    let at = v.at_stop;
    if pending_pickups(v, at) || pending_drops(v, at) {
        Phase::Port
    } else if at >= v.last_stop() {
        Phase::Arrived
    } else {
        Phase::Underway
    }
}

// ---------------------------------------------------------------------------
// spine + stop detail + nav
// ---------------------------------------------------------------------------

pub struct SpineNode {
    pub n: u32,
    pub nn: String,
    pub short: String,
    pub css: &'static str,
    pub pickups: usize,
    pub drops: usize,
}

fn spine(v: &Voyage) -> Vec<SpineNode> {
    (0..v.route.len() as u32)
        .map(|n| SpineNode {
            n,
            nn: if n == 0 { "START".into() } else { format!("STOP {n}") },
            short: short_terminal(&v.stop_name(n)),
            css: if n < v.at_stop {
                "past"
            } else if n == v.at_stop {
                "here"
            } else {
                "ahead"
            },
            pickups: v.open_loads().filter(|l| l.pickup_stop == n).count(),
            drops: v.open_loads().filter(|l| l.drop_stop == Some(n)).count(),
        })
        .collect()
}

/// `"<N>×<SIZE>+… SCU"` for one load, for the stop-detail popover.
fn box_text(l: &Load) -> String {
    let sizes: Vec<u32> = l.crates.iter().filter(|c| c.is_live()).map(|c| c.size_scu).collect();
    format!("{} SCU", format_boxes(&mix_of(sizes)))
}

/// `"off at stop 2 · Area18 TDD"` / `"bound for undecided"`.
fn route_label(l: &Load) -> String {
    match l.drop_stop {
        Some(d) => {
            let dest = l.destination.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("undecided");
            format!("off at stop {d} · {dest}")
        }
        None => "bound for undecided".into(),
    }
}

pub struct StopDetailView {
    pub n: u32,
    pub title: String,
    /// `(commodity, boxes)`.
    pub boards: Vec<(String, String)>,
    pub offs: Vec<(String, String)>,
}

fn stop_details(v: &Voyage) -> Vec<StopDetailView> {
    (0..v.route.len() as u32)
        .map(|n| StopDetailView {
            n,
            title: format!("{} · {}", if n == 0 { "START".into() } else { format!("STOP {n}") }, v.stop_name(n)),
            boards: v
                .open_loads()
                .filter(|l| l.pickup_stop == n)
                .map(|l| (l.commodity.to_uppercase(), box_text(l)))
                .collect(),
            offs: v
                .open_loads()
                .filter(|l| l.drop_stop == Some(n))
                .map(|l| (l.commodity.to_uppercase(), box_text(l)))
                .collect(),
        })
        .collect()
}

pub struct NavView {
    pub can_back: bool,
    pub prev_short: String,
    pub at_short: String,
    pub at: u32,
    pub last: u32,
    pub can_forward: bool,
    pub next_short: String,
}

fn nav_view(v: &Voyage) -> NavView {
    let can_back = v.at_stop > 0;
    let can_forward = v.at_stop < v.last_stop();
    NavView {
        can_back,
        prev_short: if can_back { short_terminal(&v.stop_name(v.at_stop - 1)) } else { String::new() },
        at_short: short_terminal(&v.stop_name(v.at_stop)),
        at: v.at_stop,
        last: v.last_stop(),
        can_forward,
        next_short: if can_forward { short_terminal(&v.stop_name(v.at_stop + 1)) } else { String::new() },
    }
}

// ---------------------------------------------------------------------------
// hold gauge
// ---------------------------------------------------------------------------

pub struct CapView {
    pub hold_scu: u32,
    pub ship_scu: i64,
    pub pct: i64,
    pub plan_scu: u32,
    pub plan_pct: i64,
    pub show_plan: bool,
    pub over: bool,
    pub high: bool,
}

fn cap_view(v: &Voyage, plan_scu: u32) -> CapView {
    let hold_scu: u32 = v
        .open_loads()
        .flat_map(|l| l.crates.iter())
        .filter(|c| c.is_live() && c.whereabouts == Whereabouts::Aboard)
        .map(|c| c.size_scu)
        .sum();
    let ship_scu = v.ship.as_ref().map(|s| s.capacity_scu).unwrap_or(0);
    let pct = if ship_scu > 0 { (hold_scu as i64 * 100 / ship_scu).min(999) } else { 0 };
    let plan_scu = plan_scu.max(hold_scu);
    let plan_pct = if ship_scu > 0 { (plan_scu as i64 * 100 / ship_scu).min(999) } else { 0 };
    CapView {
        hold_scu,
        ship_scu,
        pct,
        plan_scu,
        plan_pct,
        show_plan: plan_scu > hold_scu,
        over: ship_scu > 0 && plan_scu as i64 > ship_scu,
        high: pct >= 85,
    }
}

// ---------------------------------------------------------------------------
// Orders face
// ---------------------------------------------------------------------------

fn depth_tag(depth: Depth, len: usize) -> &'static str {
    if len < 2 {
        return "only load";
    }
    match depth {
        Depth::Deepest => "load first · deep",
        Depth::DoorSide => "by the door",
        Depth::MidHold => "mid",
    }
}

pub struct OrderSellItem {
    pub l: LoadView,
    pub n: usize,
    pub box_squares: Vec<u32>,
    /// Set down at the stop already (`where stored`), not just carried.
    pub dropped: bool,
}

pub struct OrderLoadItem {
    pub l: LoadView,
    pub n: usize,
    pub box_squares: Vec<u32>,
    pub depth_tag: &'static str,
    pub route_label: String,
    /// Boards here and isn't aboard yet: offers `[Stow]`.
    pub waiting: bool,
    /// Boards here (whether or not it's aboard yet): offers `[Unstow]` once stowed.
    pub picked_up_here: bool,
}

pub struct SealView {
    /// `none` (nothing to handle) / `ready` (cleared) / `open` (work left).
    pub kind: &'static str,
    pub open_count: usize,
}

fn load_by_id(v: &Voyage, id: i64) -> Option<&Load> {
    v.loads.iter().find(|l| l.id == id)
}

/// What comes off here, door-first: the leg ending at `at`'s aboard set,
/// filtered to this stop's drops, reversed (that leg lists deepest first).
fn sell_due_items(v: &Voyage, legs: &[Leg], at: u32) -> Vec<OrderSellItem> {
    let Some(leg) = legs.iter().find(|l| l.to == Some(at)) else { return Vec::new() };
    let mut entries: Vec<&LegEntry> = leg.aboard.iter().filter(|e| e.entry.drop_stop == Some(at)).collect();
    entries.reverse();
    entries
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let load = load_by_id(v, e.entry.load_id)?;
            let l = LoadView::new(load, v);
            Some(OrderSellItem {
                n: i + 1,
                box_squares: expand_boxes(&l.boxes),
                dropped: load.status() == LoadStatus::Stored,
                l,
            })
        })
        .collect()
}

/// The hold as it will sit leaving here, deepest first: the leg starting at
/// `at`'s aboard set, already in that order.
fn load_seq_items(v: &Voyage, legs: &[Leg], at: u32) -> Vec<OrderLoadItem> {
    let Some(leg) = legs.iter().find(|l| l.from == at) else { return Vec::new() };
    let len = leg.aboard.len();
    leg.aboard
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let load = load_by_id(v, e.entry.load_id)?;
            let l = LoadView::new(load, v);
            let picked_up_here = e.entry.pickup_stop == at;
            Some(OrderLoadItem {
                n: i + 1,
                box_squares: expand_boxes(&l.boxes),
                depth_tag: depth_tag(e.entry.depth, len),
                route_label: route_label(load),
                waiting: picked_up_here && load.status() != LoadStatus::InTransit,
                picked_up_here,
                l,
            })
        })
        .collect()
}

fn seal(sell_due: &[OrderSellItem], load_seq: &[OrderLoadItem]) -> SealView {
    let open_count = sell_due.len() + load_seq.iter().filter(|i| i.waiting).count();
    let kind = if sell_due.is_empty() && load_seq.is_empty() {
        "none"
    } else if open_count == 0 {
        "ready"
    } else {
        "open"
    };
    SealView { kind, open_count }
}

// ---------------------------------------------------------------------------
// Tracker face
// ---------------------------------------------------------------------------

pub struct AboardItem {
    pub l: LoadView,
    pub depth_tag: &'static str,
    pub route_label: String,
}

fn aboard_now(v: &Voyage) -> Vec<AboardItem> {
    let mut loads: Vec<&Load> = v.open_loads().filter(|l| l.status() == LoadStatus::InTransit).collect();
    loads.sort_by_key(|l| off_key(l.drop_stop));
    let n = loads.len();
    loads
        .into_iter()
        .enumerate()
        .map(|(i, l)| AboardItem {
            depth_tag: if i == 0 {
                "by the door"
            } else if i + 1 == n {
                "deep"
            } else {
                "mid"
            },
            route_label: route_label(l),
            l: LoadView::new(l, v),
        })
        .collect()
}

pub struct HereRow {
    pub dir: &'static str,
    pub commodity_upper: String,
    pub boxes: String,
    pub state: &'static str,
}

fn here_rows(v: &Voyage, at: u32) -> Vec<HereRow> {
    let mut rows: Vec<HereRow> = v
        .open_loads()
        .filter(|l| l.drop_stop == Some(at))
        .map(|l| HereRow {
            dir: "down",
            commodity_upper: l.commodity.to_uppercase(),
            boxes: box_text(l),
            state: if l.status() == LoadStatus::Stored { "at station" } else { "to drop" },
        })
        .collect();
    rows.extend(v.open_loads().filter(|l| l.pickup_stop == at).map(|l| HereRow {
        dir: "up",
        commodity_upper: l.commodity.to_uppercase(),
        boxes: box_text(l),
        state: if l.status() == LoadStatus::InTransit { "aboard" } else { "to stow" },
    }));
    rows
}

pub struct NextView {
    pub is_final: bool,
    pub short: String,
    pub drops: Vec<(String, String)>,
    pub pickups: Vec<(String, String)>,
}

fn next_view(v: &Voyage) -> NextView {
    match v.next_stop() {
        None => NextView { is_final: true, short: String::new(), drops: Vec::new(), pickups: Vec::new() },
        Some(n) => NextView {
            is_final: false,
            short: short_terminal(&v.stop_name(n)),
            drops: v.open_loads().filter(|l| l.drop_stop == Some(n)).map(|l| (l.commodity.to_uppercase(), box_text(l))).collect(),
            pickups: v.open_loads().filter(|l| l.pickup_stop == n).map(|l| (l.commodity.to_uppercase(), box_text(l))).collect(),
        },
    }
}

// ---------------------------------------------------------------------------
// the card
// ---------------------------------------------------------------------------

pub struct VoyageCardView {
    pub id: i64,
    pub label: String,
    pub ship_name: String,
    pub crew_line: String,
    pub load_count: usize,
    pub phase_label: &'static str,
    pub phase_css: &'static str,
    pub auto_face: &'static str,
    pub face_label: &'static str,
    pub other_face_label: &'static str,
    pub spine: Vec<SpineNode>,
    pub stops: Vec<StopDetailView>,
    pub nav: NavView,
    pub cap: CapView,
    pub sell_due: Vec<OrderSellItem>,
    pub load_seq: Vec<OrderLoadItem>,
    pub seal: SealView,
    pub aboard: Vec<AboardItem>,
    pub here_rows: Vec<HereRow>,
    pub here_pending: bool,
    pub next: NextView,
}

fn voyage_card(v: &Voyage) -> VoyageCardView {
    let phase = phase_of(v);
    let legs = v.legs();
    let at = v.at_stop;
    let sell_due = sell_due_items(v, &legs, at);
    let load_seq = load_seq_items(v, &legs, at);
    let plan_scu: u32 = load_seq.iter().map(|i| i.l.scu).sum();
    let open = v.open_loads().count();
    VoyageCardView {
        id: v.id,
        label: format!("Voyage {}", v.id),
        ship_name: v.hull_label.clone().or_else(|| v.ship.as_ref().map(|s| s.name.clone())).unwrap_or_else(|| "—".into()),
        crew_line: crate::stowage::views::crew_line(v),
        load_count: open,
        phase_label: phase.label(),
        phase_css: phase.css(),
        auto_face: phase.face(),
        face_label: phase.face_label(),
        other_face_label: phase.other_face_label(),
        spine: spine(v),
        stops: stop_details(v),
        nav: nav_view(v),
        cap: cap_view(v, plan_scu),
        seal: seal(&sell_due, &load_seq),
        sell_due,
        load_seq,
        aboard: aboard_now(v),
        here_pending: pending_pickups(v, at) || pending_drops(v, at),
        here_rows: here_rows(v, at),
        next: next_view(v),
    }
}

/// One card per open voyage, same order as the Ledger's Open Cargo table.
pub fn route_ops_cards(voyages: &[Voyage]) -> Vec<VoyageCardView> {
    voyages.iter().filter(|v| v.open_loads().next().is_some()).map(voyage_card).collect()
}
