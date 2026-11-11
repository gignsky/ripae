//! Voyages, loads, crates and shares -- the 0.2 "Stowage" data model and
//! every derived figure from `RIPAE-0.2.md` §4. Nothing here touches the
//! database; `repo.rs` loads these and the handlers render them.
//!
//! The derived figures (status, where-now, cost, write-offs, sale profit,
//! shares, leaderboard credit, stowage order, sc-cargo link) are the parts
//! the brief wants under test before any handler exists; the tests at the
//! bottom are that specification.

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

/// The seven crate sizes sc-cargo.space knows, in the order its URL grammar
/// spells them (`q w e r t y u`).
pub const CRATE_SIZES: [u32; 7] = [1, 2, 4, 8, 16, 24, 32];
const SIZE_KEYS: [char; 7] = ['q', 'w', 'e', 'r', 't', 'y', 'u'];

// ---------------------------------------------------------------------------
// enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Sound,
    Glitched,
    Lost,
    Sold,
}

impl Condition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Condition::Sound => "sound",
            Condition::Glitched => "glitched",
            Condition::Lost => "lost",
            Condition::Sold => "sold",
        }
    }

    /// Tap on a crate tile: sound → glitched → lost → sound. Sold crates
    /// don't cycle.
    pub fn next(&self) -> Condition {
        match self {
            Condition::Sound => Condition::Glitched,
            Condition::Glitched => Condition::Lost,
            Condition::Lost => Condition::Sound,
            Condition::Sold => Condition::Sold,
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Condition {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sound" => Ok(Condition::Sound),
            "glitched" => Ok(Condition::Glitched),
            "lost" => Ok(Condition::Lost),
            "sold" => Ok(Condition::Sold),
            other => Err(format!("unknown crate condition: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whereabouts {
    AwaitingPickup,
    Aboard,
    Stored,
}

impl Whereabouts {
    pub fn as_str(&self) -> &'static str {
        match self {
            Whereabouts::AwaitingPickup => "awaiting_pickup",
            Whereabouts::Aboard => "aboard",
            Whereabouts::Stored => "stored",
        }
    }
}

impl fmt::Display for Whereabouts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Whereabouts {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "awaiting_pickup" => Ok(Whereabouts::AwaitingPickup),
            "aboard" => Ok(Whereabouts::Aboard),
            "stored" => Ok(Whereabouts::Stored),
            other => Err(format!("unknown whereabouts: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceMode {
    Scu,
    Lot,
}

impl PriceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PriceMode::Scu => "scu",
            PriceMode::Lot => "lot",
        }
    }
}

impl FromStr for PriceMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "scu" => Ok(PriceMode::Scu),
            "lot" => Ok(PriceMode::Lot),
            other => Err(format!("unknown price mode: {other}")),
        }
    }
}

/// Commodity cargo bought at a terminal, or the boxes of a hauling contract:
/// picked up for nothing and paid for on delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadKind {
    #[default]
    Trade,
    Haul,
}

impl LoadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoadKind::Trade => "trade",
            LoadKind::Haul => "haul",
        }
    }
}

impl FromStr for LoadKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trade" => Ok(LoadKind::Trade),
            "haul" => Ok(LoadKind::Haul),
            other => Err(format!("unknown load kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    Pct,
    Flat,
}

impl ShareKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShareKind::Pct => "pct",
            ShareKind::Flat => "flat",
        }
    }
}

impl FromStr for ShareKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pct" => Ok(ShareKind::Pct),
            "flat" => Ok(ShareKind::Flat),
            other => Err(format!("unknown share kind: {other}")),
        }
    }
}

/// Derived, never stored: where a load's crates agree, that; where they
/// differ, `Split`; once every crate is sold or lost, `Sold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    AwaitingPickup,
    InTransit,
    Stored,
    Split,
    Sold,
}

impl LoadStatus {
    /// Chip text, as the mock spells it.
    pub fn label(&self) -> &'static str {
        match self {
            LoadStatus::AwaitingPickup => "Awaiting pickup",
            LoadStatus::InTransit => "In transit",
            LoadStatus::Stored => "Stored",
            LoadStatus::Split => "Split",
            LoadStatus::Sold => "Sold",
        }
    }

    /// CSS modifier on `.status` / the row (mock: transit, stored, pickup, split).
    pub fn css(&self) -> &'static str {
        match self {
            LoadStatus::AwaitingPickup => "pickup",
            LoadStatus::InTransit => "transit",
            LoadStatus::Stored => "stored",
            LoadStatus::Split => "split",
            LoadStatus::Sold => "sold",
        }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self, LoadStatus::Sold)
    }
}

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Crate {
    pub id: i64,
    pub load_id: i64,
    pub size_scu: u32,
    pub condition: Condition,
    pub whereabouts: Whereabouts,
    /// Station, when `whereabouts == Stored`.
    pub stored_at: Option<String>,
    /// Set when `condition == Sold`.
    pub sold_at: Option<String>,
}

impl Crate {
    pub fn is_lost(&self) -> bool {
        self.condition == Condition::Lost
    }
    pub fn is_sold(&self) -> bool {
        self.condition == Condition::Sold
    }
    /// Still in play: neither lost nor sold.
    pub fn is_live(&self) -> bool {
        !self.is_lost() && !self.is_sold()
    }
    /// Can be picked in a close: sound or glitched (ruled: glitched by hand).
    pub fn is_sellable(&self) -> bool {
        self.is_live()
    }
    /// Pre-ticked by `[ALL SOUND CRATES]`: sound only.
    pub fn default_selected(&self) -> bool {
        self.condition == Condition::Sound
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub id: i64,
    pub load_id: i64,
    /// Who earned it -- keeps the leaderboard credit.
    pub to_user: i64,
    pub to_user_name: String,
    /// Who receives the aUEC; equals `to_user` unless forfeited.
    pub paid_to: i64,
    pub paid_to_name: String,
    pub kind: ShareKind,
    /// Percent (0–100) or aUEC.
    pub amount: f64,
    pub reason: Option<String>,
    /// Frozen at close; 0 on a losing sale, both kinds.
    pub paid_auec: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Load {
    pub id: i64,
    pub voyage_id: i64,
    pub submitted_by: i64,
    pub submitted_by_name: String,
    /// Who bought it; unlike `submitted_by` it doesn't become the seller at close.
    pub bought_by: i64,
    pub bought_by_name: String,
    pub kind: LoadKind,
    pub commodity: String,
    /// The pickup, on a haul.
    pub buy_terminal: String,
    pub price_mode: PriceMode,
    pub buy_price_scu: f64,
    pub buy_price_lot: Option<f64>,
    pub stop_no: u32,
    pub destination: Option<String>,
    pub sold_at: Option<String>,
    pub sell_terminal: Option<String>,
    pub sell_price_scu: Option<f64>,
    pub sell_price_lot: Option<f64>,
    /// A haul's promised payout; only the delivery's recorded price counts.
    pub reward: Option<f64>,
    pub notes: Option<String>,
    pub crates: Vec<Crate>,
    pub shares: Vec<Share>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ship {
    pub id: i64,
    pub manufacturer: String,
    pub name: String,
    pub short: String,
    pub sc_cargo_slug: String,
    pub capacity_scu: i64,
    pub concept: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Voyage {
    pub id: i64,
    pub logged_by: i64,
    pub logged_by_name: String,
    /// Who typed it in (the bookkeeper), shown as "Logged by"; `logged_by`
    /// is whose run it is.
    pub entered_by: i64,
    pub entered_by_name: String,
    pub ship: Option<Ship>,
    pub bought_at: String,
    pub notes: Option<String>,
    pub loads: Vec<Load>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub handle: String,
    pub display_name: String,
    /// `users.created_at` -- the "aboard since" date on the service record.
    pub created_at: String,
    /// Their Discord avatar, for the service record; `None` before Discord.
    pub avatar_url: Option<String>,
}

// ---------------------------------------------------------------------------
// derived figures on a load
// ---------------------------------------------------------------------------

impl Load {
    /// `'sold' if every crate sold/lost; else the one whereabouts all live
    /// crates share; else 'split'`.
    pub fn status(&self) -> LoadStatus {
        let mut live = self.crates.iter().filter(|c| c.is_live()).map(|c| c.whereabouts);
        let Some(first) = live.next() else {
            return LoadStatus::Sold;
        };
        if live.all(|w| w == first) {
            match first {
                Whereabouts::Aboard => LoadStatus::InTransit,
                Whereabouts::Stored => LoadStatus::Stored,
                Whereabouts::AwaitingPickup => LoadStatus::AwaitingPickup,
            }
        } else {
            LoadStatus::Split
        }
    }

    pub fn is_open(&self) -> bool {
        self.status().is_open()
    }

    pub fn is_sold(&self) -> bool {
        !self.is_open()
    }

    pub fn is_haul(&self) -> bool {
        self.kind == LoadKind::Haul
    }

    /// Closed by an actual sale -- as opposed to every crate lost or written
    /// off, which also takes a load out of Open Cargo but earns nothing and
    /// has no place in the ledger.
    pub fn was_sold(&self) -> bool {
        self.sold_at.is_some()
    }

    /// Marks every glitched crate not in `keep` lost, as a close does when
    /// the unsold glitched cargo is written off. Returns their ids.
    pub fn write_off_glitched(&mut self, keep: &[i64]) -> Vec<i64> {
        let mut ids = Vec::new();
        for c in &mut self.crates {
            if c.condition == Condition::Glitched && !keep.contains(&c.id) {
                c.condition = Condition::Lost;
                ids.push(c.id);
            }
        }
        ids
    }

    /// `"n aboard <owner>'s <ship> · m stored at <station> · k awaiting pickup"`,
    /// only the non-zero parts. `hull` is the voyage's `"<owner>'s <ship>"`,
    /// or `None` when no hull is assigned yet. When every live crate is in
    /// one place the count is dropped (`"aboard JT's Hull-C"`,
    /// `"at Everus Harbor"`).
    pub fn where_now(&self, hull: Option<&str>) -> String {
        let live: Vec<&Crate> = self.crates.iter().filter(|c| c.is_live()).collect();
        if live.is_empty() {
            return "sold".to_string();
        }
        let aboard = live.iter().filter(|c| c.whereabouts == Whereabouts::Aboard).count();
        let pickup = live
            .iter()
            .filter(|c| c.whereabouts == Whereabouts::AwaitingPickup)
            .count();
        // stored, grouped by station in first-seen order
        let mut stations: Vec<(String, usize)> = Vec::new();
        for c in live.iter().filter(|c| c.whereabouts == Whereabouts::Stored) {
            let at = c.stored_at.clone().unwrap_or_else(|| "unknown station".into());
            match stations.iter_mut().find(|(s, _)| *s == at) {
                Some((_, n)) => *n += 1,
                None => stations.push((at, 1)),
            }
        }
        let groups = (aboard > 0) as usize + (pickup > 0) as usize + stations.len();
        let hull_text = hull.map(|h| format!("aboard {h}")).unwrap_or_else(|| "aboard · no hull yet".into());
        if groups == 1 {
            if aboard > 0 {
                return hull_text;
            }
            if pickup > 0 {
                if self.buy_terminal.is_empty() {
                    return "awaiting pickup".into();
                }
                return format!("awaiting pickup at {}", self.buy_terminal);
            }
            return format!("at {}", stations[0].0);
        }
        let mut parts = Vec::new();
        if aboard > 0 {
            parts.push(format!("{aboard} {hull_text}"));
        }
        for (at, n) in &stations {
            parts.push(format!("{n} stored at {at}"));
        }
        if pickup > 0 {
            parts.push(format!("{pickup} awaiting pickup"));
        }
        parts.join(" · ")
    }

    /// Σ size over crates that aren't lost (sold ones included).
    pub fn scu(&self) -> u32 {
        self.crates.iter().filter(|c| !c.is_lost()).map(|c| c.size_scu).sum()
    }

    /// Σ size over every crate, lost included.
    pub fn scu_all(&self) -> u32 {
        self.crates.iter().map(|c| c.size_scu).sum()
    }

    pub fn scu_lost(&self) -> u32 {
        self.crates.iter().filter(|c| c.is_lost()).map(|c| c.size_scu).sum()
    }

    /// Σ size over sold crates.
    pub fn scu_sold(&self) -> u32 {
        self.crates.iter().filter(|c| c.is_sold()).map(|c| c.size_scu).sum()
    }

    /// `cost = scu(all crates incl. lost) × buy_price_scu`.
    pub fn cost(&self) -> f64 {
        self.scu_all() as f64 * self.buy_price_scu
    }

    /// `written_off = Σ size over lost crates × buy_price_scu`.
    pub fn written_off(&self) -> f64 {
        self.scu_lost() as f64 * self.buy_price_scu
    }

    /// Revenue of a sold load: the lot figure when priced per lot, else
    /// `scu_sold × sell_price_scu`. `None` while open.
    pub fn revenue(&self) -> Option<f64> {
        if !self.is_sold() || self.scu_sold() == 0 && self.sell_price_scu.is_none() {
            return None;
        }
        match (self.sell_price_lot, self.sell_price_scu) {
            (Some(lot), _) => Some(lot),
            (None, Some(per)) => Some(self.scu_sold() as f64 * per),
            (None, None) => None,
        }
    }

    /// `cost_sold = scu_sold × buy_price_scu` -- only the crates that sold.
    pub fn cost_sold(&self) -> Option<f64> {
        self.is_sold().then(|| self.scu_sold() as f64 * self.buy_price_scu)
    }

    pub fn profit(&self) -> Option<f64> {
        Some(self.revenue()? - self.cost_sold()?)
    }

    /// `profit / cost_sold`, 0 when cost is 0. `None` while open.
    pub fn margin(&self) -> Option<f64> {
        let cost = self.cost_sold()?;
        let profit = self.profit()?;
        Some(if cost == 0.0 { 0.0 } else { profit / cost })
    }

    pub fn shares_paid(&self) -> f64 {
        self.shares.iter().map(|s| s.paid_auec).sum()
    }

    /// `seller_keeps = profit − Σ share.paid`.
    pub fn seller_keeps(&self) -> Option<f64> {
        Some(self.profit()? - self.shares_paid())
    }

    /// Crate mix of the non-lost crates as `(count, size)`, largest first.
    pub fn boxes(&self) -> BoxMix {
        mix_of(self.crates.iter().filter(|c| !c.is_lost()).map(|c| c.size_scu))
    }

    pub fn lost_count(&self) -> usize {
        self.crates.iter().filter(|c| c.is_lost()).count()
    }

    pub fn glitched_count(&self) -> usize {
        self.crates
            .iter()
            .filter(|c| c.condition == Condition::Glitched)
            .count()
    }
}

/// Buy or sell: the figure entered and the one derived from it, both kept.
/// Per SCU → lot is `price × scu`; per lot → per SCU is `price / scu`
/// (0 when there's no SCU to divide by).
pub fn derive_prices(mode: PriceMode, price: f64, scu: u32) -> (f64, f64) {
    match mode {
        PriceMode::Scu => (price, price * scu as f64),
        PriceMode::Lot => (if scu == 0 { 0.0 } else { price / scu as f64 }, price),
    }
}

// ---------------------------------------------------------------------------
// closing a load
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ShareInput {
    pub to_user: i64,
    /// Forfeit the aUEC to this member; `None` = the earner keeps it.
    pub paid_to: Option<i64>,
    pub kind: ShareKind,
    pub amount: f64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaleInput {
    /// Ids of the crates that sold. Lost crates are refused; unknown ids too.
    pub crate_ids: Vec<i64>,
    pub price_mode: PriceMode,
    pub price: f64,
    pub shares: Vec<ShareInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettledShare {
    pub to_user: i64,
    pub paid_to: i64,
    pub kind: ShareKind,
    pub amount: f64,
    pub reason: Option<String>,
    pub paid: f64,
}

/// Everything a close works out before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Sale {
    /// The crates that sold, still carrying their pre-sale state.
    pub sold: Vec<Crate>,
    /// The crates that stay open on the voyage (lost ones stay here too).
    pub kept: Vec<Crate>,
    /// Glitched crates this close writes off as lost (see
    /// `Load::write_off_glitched`); they sit in `kept`, already lost.
    pub written_off: Vec<i64>,
    pub scu_sold: u32,
    pub sell_price_scu: f64,
    pub sell_price_lot: f64,
    pub revenue: f64,
    pub cost_sold: f64,
    pub profit: f64,
    pub shares: Vec<SettledShare>,
    pub seller_keeps: f64,
}

impl Sale {
    /// True when nothing sellable is left behind -- the load closes whole.
    pub fn closes_load(&self) -> bool {
        self.kept.iter().all(|c| !c.is_live())
    }
}

/// `share.paid = 0 when profit ≤ 0 (ruled: nothing is owed on a loss, flat
/// fees included) else profit × amount/100 (pct) | amount (flat)`.
pub fn share_paid(profit: f64, kind: ShareKind, amount: f64) -> f64 {
    if profit <= 0.0 {
        return 0.0;
    }
    match kind {
        ShareKind::Pct => profit * amount / 100.0,
        ShareKind::Flat => amount,
    }
}

/// Works out a sale against a load without touching it: which crates go,
/// which stay, the money, and what every share pays.
pub fn settle(load: &Load, input: &SaleInput) -> Result<Sale, String> {
    if input.crate_ids.is_empty() {
        return Err("Pick at least one crate to sell.".into());
    }
    if input.price < 0.0 || !input.price.is_finite() {
        return Err("Sell price can't be negative.".into());
    }
    let mut sold = Vec::new();
    let mut kept = Vec::new();
    for c in &load.crates {
        if input.crate_ids.contains(&c.id) {
            if !c.is_sellable() {
                return Err(format!(
                    "Crate #{} is {} and can't be sold.",
                    c.id, c.condition
                ));
            }
            sold.push(c.clone());
        } else {
            kept.push(c.clone());
        }
    }
    if sold.len() != input.crate_ids.iter().collect::<std::collections::HashSet<_>>().len() {
        return Err("One of those crates isn't on this load.".into());
    }
    let scu_sold: u32 = sold.iter().map(|c| c.size_scu).sum();
    let (sell_price_scu, sell_price_lot) = derive_prices(input.price_mode, input.price, scu_sold);
    let revenue = sell_price_lot;
    let cost_sold = scu_sold as f64 * load.buy_price_scu;
    let profit = revenue - cost_sold;
    let shares = settle_shares(profit, &input.shares)?;
    let paid: f64 = shares.iter().map(|s| s.paid).sum();
    Ok(Sale {
        sold,
        kept,
        written_off: Vec::new(),
        scu_sold,
        sell_price_scu,
        sell_price_lot,
        revenue,
        cost_sold,
        profit,
        shares,
        seller_keeps: profit - paid,
    })
}

/// What every share pays on `profit`, refusing negative amounts and
/// percentages that add up to more than the whole.
fn settle_shares(profit: f64, inputs: &[ShareInput]) -> Result<Vec<SettledShare>, String> {
    let mut pct_total = 0.0;
    let mut shares = Vec::new();
    for s in inputs {
        if s.amount < 0.0 || !s.amount.is_finite() {
            return Err("Share amounts can't be negative.".into());
        }
        if s.kind == ShareKind::Pct {
            pct_total += s.amount;
        }
        shares.push(SettledShare {
            to_user: s.to_user,
            paid_to: s.paid_to.unwrap_or(s.to_user),
            kind: s.kind,
            amount: s.amount,
            reason: s.reason.clone(),
            paid: share_paid(profit, s.kind, s.amount),
        });
    }
    if pct_total > 100.0 + 1e-9 {
        return Err(format!("Percentage shares add up to {pct_total}% -- more than the whole profit."));
    }
    Ok(shares)
}

/// A recorded sale re-priced: the same crates, a corrected price and shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Resale {
    pub sell_price_scu: f64,
    pub sell_price_lot: f64,
    pub profit: f64,
    pub shares: Vec<SettledShare>,
}

/// Works out a correction to a sale already on the books. Which crates sold
/// doesn't change -- only the price, and the shares paid on the new profit.
pub fn resettle(load: &Load, price_mode: PriceMode, price: f64, shares: &[ShareInput]) -> Result<Resale, String> {
    if !load.was_sold() {
        return Err("That load hasn't been sold.".into());
    }
    if price < 0.0 || !price.is_finite() {
        return Err("Sell price can't be negative.".into());
    }
    let scu_sold = load.scu_sold();
    let (sell_price_scu, sell_price_lot) = derive_prices(price_mode, price, scu_sold);
    let profit = sell_price_lot - scu_sold as f64 * load.buy_price_scu;
    Ok(Resale { sell_price_scu, sell_price_lot, profit, shares: settle_shares(profit, shares)? })
}

// ---------------------------------------------------------------------------
// totals and the leaderboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totals {
    /// Σ cost over open loads, before write-offs.
    pub committed_gross: f64,
    /// Σ written_off over open loads.
    pub written_off: f64,
    /// `committed = Σ cost over open loads − written_off`.
    pub committed: f64,
    /// Σ written_off over every load (open, sold or written off whole) in a
    /// closed voyage (`Voyage::is_fully_sold`). Write-offs in a voyage that is
    /// still open stay out of it until that voyage closes.
    pub losses: f64,
    /// `realised = Σ profit over sold loads − losses`: lost cargo was paid
    /// for, so once its voyage closes it comes off the overall profit.
    pub realised: f64,
    pub open_voyages: usize,
    pub open_loads: usize,
    pub open_scu: u32,
}

impl Totals {
    pub fn from_voyages<'a>(voyages: impl IntoIterator<Item = &'a Voyage>) -> Self {
        let mut t = Totals::default();
        for v in voyages {
            let mut any_open = false;
            let closed = v.is_fully_sold();
            for l in &v.loads {
                if closed {
                    t.losses += l.written_off();
                }
                if l.is_open() {
                    any_open = true;
                    t.open_loads += 1;
                    t.open_scu += l.scu();
                    t.committed_gross += l.cost();
                    t.written_off += l.written_off();
                } else if let Some(p) = l.profit() {
                    t.realised += p;
                }
            }
            if any_open {
                t.open_voyages += 1;
            }
        }
        t.committed = t.committed_gross - t.written_off;
        t.realised -= t.losses;
        t
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub from: String,
    pub to: String,
    pub profit: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    pub user_id: i64,
    pub handle: String,
    pub display_name: String,
    /// Net of shares: kept as seller + shares earned (forfeited or not).
    pub board_credit: f64,
    /// aUEC actually received: sale revenue as seller, less the shares paid
    /// out, plus shares paid to this member (see [`cash`]).
    pub cash: f64,
    /// Shares this member earned but forfeited to someone else.
    pub forfeited: f64,
    /// Shares other members earned and forfeited to this one.
    pub forfeits_received: f64,
    /// Voyages with every load sold, where this member took part in one
    /// (sold it or held a share).
    pub runs_closed: usize,
    pub loads_sold: usize,
    pub scu_hauled: u32,
    pub best_route: Option<Route>,
    /// This member's share of the org on the board's sort key, 0–100: see
    /// [`standings`].
    pub share_of_org: f64,
}

impl Standing {
    /// Nothing sold, nothing shared, nothing lost: the row is dimmed.
    pub fn is_idle(&self) -> bool {
        self.loads_sold == 0 && self.board_credit == 0.0 && self.cash == 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Profit,
    Collected,
    Runs,
    Scu,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Profit, SortKey::Collected, SortKey::Runs, SortKey::Scu];

    pub fn parse(s: &str) -> SortKey {
        match s.trim().to_ascii_lowercase().as_str() {
            "collected" | "cash" => SortKey::Collected,
            "runs" => SortKey::Runs,
            "scu" => SortKey::Scu,
            _ => SortKey::Profit,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            SortKey::Profit => "profit",
            SortKey::Collected => "collected",
            SortKey::Runs => "runs",
            SortKey::Scu => "scu",
        }
    }
}

/// `board_credit(u) = Σ seller_keeps where u sold + Σ share.paid where
/// share.to_user = u − losses(u)`.
pub fn board_credit(user_id: i64, voyages: &[Voyage]) -> f64 {
    let sales: f64 = sold_loads(voyages)
        .map(|l| {
            let kept = if l.submitted_by == user_id { l.seller_keeps().unwrap_or(0.0) } else { 0.0 };
            let earned: f64 = l.shares.iter().filter(|s| s.to_user == user_id).map(|s| s.paid_auec).sum();
            kept + earned
        })
        .sum();
    sales - losses(user_id, voyages)
}

/// `cash(u) = Σ (revenue − shares_paid) where u sold + Σ share.paid where
/// share.paid_to = u`: the aUEC that landed in the member's wallet. What the
/// cargo cost, lost crates included, doesn't come off it -- that's profit.
pub fn cash(user_id: i64, voyages: &[Voyage]) -> f64 {
    sold_loads(voyages)
        .map(|l| {
            let taken = if l.submitted_by == user_id { l.revenue().unwrap_or(0.0) - l.shares_paid() } else { 0.0 };
            let received: f64 = l.shares.iter().filter(|s| s.paid_to == user_id).map(|s| s.paid_auec).sum();
            taken + received
        })
        .sum()
}

/// `losses(u) = Σ written_off over every load u holds in a closed voyage`
/// (open, sold or written off whole): whoever's cargo it was paid for the
/// crates that were lost. Like `Totals::losses`, write-offs in a voyage that
/// is still open land only once it closes. Shares aren't touched -- they're
/// settled on the sale.
pub fn losses(user_id: i64, voyages: &[Voyage]) -> f64 {
    voyages
        .iter()
        .filter(|v| v.is_fully_sold())
        .flat_map(|v| v.loads.iter())
        .filter(|l| l.submitted_by == user_id)
        .map(|l| l.written_off())
        .sum()
}

/// Every sold load across the ledger; `submitted_by` on a sold row is the
/// member who closed it (the seller).
fn sold_loads(voyages: &[Voyage]) -> impl Iterator<Item = &Load> {
    voyages.iter().flat_map(|v| v.loads.iter()).filter(|l| l.was_sold())
}

/// A member took part in a sold load when they sold it or hold a share of
/// its profit -- a crewmate on 50 % flew the route as much as the seller.
fn took_part(l: &Load, user_id: i64) -> bool {
    l.submitted_by == user_id || l.shares.iter().any(|s| s.to_user == user_id)
}

/// Voyages with every load sold where `user_id` took part in at least one of them.
fn runs_closed_by(user_id: i64, voyages: &[Voyage]) -> usize {
    voyages
        .iter()
        .filter(|v| v.is_fully_sold() && v.loads.iter().any(|l| took_part(l, user_id)))
        .count()
}

fn best_route_of(user_id: i64, voyages: &[Voyage]) -> Option<Route> {
    sold_loads(voyages)
        .filter(|l| took_part(l, user_id) && !l.buy_terminal.is_empty())
        .filter_map(|l| {
            Some(Route {
                from: l.buy_terminal.clone(),
                to: l.sell_terminal.clone()?,
                profit: l.profit()?,
            })
        })
        .max_by(|a, b| a.profit.total_cmp(&b.profit))
}

/// `forfeited(u) = Σ share.paid where share.to_user = u and share.paid_to ≠ u`
/// and `forfeits_received(u)` the other way round: what a forfeit moved
/// between earner and receiver.
fn forfeits(user_id: i64, voyages: &[Voyage]) -> (f64, f64) {
    sold_loads(voyages).flat_map(|l| l.shares.iter()).fold((0.0, 0.0), |(out, inn), s| {
        match (s.to_user == user_id, s.paid_to == user_id) {
            (true, false) => (out + s.paid_auec, inn),
            (false, true) => (out, inn + s.paid_auec),
            _ => (out, inn),
        }
    })
}

/// `part / whole × 100`, clamped to 0–100; 0 when there is no whole.
fn pct(part: f64, whole: f64) -> f64 {
    if whole > 0.0 { (part.max(0.0) / whole * 100.0).min(100.0) } else { 0.0 }
}

/// One row per user, sorted by `key` (descending). Ties fall back, in
/// order, to profit, collected, runs, SCU, then display name (A→Z,
/// case-insensitive) and id, so the order never depends on input order.
///
/// `share_of_org` follows the key. Profit and collected are a share of the
/// org's positive total (negatives count as 0), so they add up to 100 %.
/// Runs and SCU are the fraction of the org's closed runs / SCU sold that
/// the member took part in: a shared run counts for everyone aboard, so
/// those don't add up to 100 %, but none ever exceeds it.
pub fn standings(users: &[User], voyages: &[Voyage], key: SortKey) -> Vec<Standing> {
    let mut rows: Vec<Standing> = users
        .iter()
        .map(|u| {
            let mine: Vec<&Load> = sold_loads(voyages).filter(|l| took_part(l, u.id)).collect();
            let (forfeited, forfeits_received) = forfeits(u.id, voyages);
            Standing {
                user_id: u.id,
                handle: u.handle.clone(),
                display_name: u.display_name.clone(),
                board_credit: board_credit(u.id, voyages),
                cash: cash(u.id, voyages),
                forfeited,
                forfeits_received,
                runs_closed: runs_closed_by(u.id, voyages),
                loads_sold: mine.len(),
                scu_hauled: mine.iter().map(|l| l.scu_sold()).sum(),
                best_route: best_route_of(u.id, voyages),
                share_of_org: 0.0,
            }
        })
        .collect();
    let whole = match key {
        SortKey::Profit => rows.iter().map(|r| r.board_credit.max(0.0)).sum(),
        SortKey::Collected => rows.iter().map(|r| r.cash.max(0.0)).sum(),
        SortKey::Runs => voyages.iter().filter(|v| v.is_fully_sold()).count() as f64,
        SortKey::Scu => sold_loads(voyages).map(|l| l.scu_sold() as f64).sum(),
    };
    for r in &mut rows {
        let part = match key {
            SortKey::Profit => r.board_credit,
            SortKey::Collected => r.cash,
            SortKey::Runs => r.runs_closed as f64,
            SortKey::Scu => r.scu_hauled as f64,
        };
        r.share_of_org = pct(part, whole);
    }
    rows.sort_by(|a, b| {
        let primary = match key {
            SortKey::Profit => b.board_credit.total_cmp(&a.board_credit),
            SortKey::Collected => b.cash.total_cmp(&a.cash),
            SortKey::Runs => b.runs_closed.cmp(&a.runs_closed),
            SortKey::Scu => b.scu_hauled.cmp(&a.scu_hauled),
        };
        primary
            .then(b.board_credit.total_cmp(&a.board_credit))
            .then(b.cash.total_cmp(&a.cash))
            .then(b.runs_closed.cmp(&a.runs_closed))
            .then(b.scu_hauled.cmp(&a.scu_hauled))
            .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
            .then(a.user_id.cmp(&b.user_id))
    });
    rows
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceRecord {
    pub user: User,
    pub board_credit: f64,
    pub cash: f64,
    pub runs_closed: usize,
    pub loads_sold: usize,
    pub scu_hauled: u32,
    /// `(commodity, profit)`
    pub best_trade: Option<(String, f64)>,
    pub worst_trade: Option<(String, f64)>,
    /// `(commodity, loads)`
    pub favourite_commodity: Option<(String, usize)>,
    /// `(from, to, times)`
    pub favourite_route: Option<(String, String, usize)>,
    pub crates_lost: usize,
    pub scu_lost: u32,
    /// What those lost crates cost, open voyages included; only the part on
    /// closed voyages (`losses`) is off `board_credit` so far.
    pub written_off: f64,
    pub shares_paid_out: f64,
    pub shares_received: f64,
    pub ships_flown: Vec<String>,
    pub first_sale: Option<String>,
    pub latest_sale: Option<String>,
    /// 1-based rank by profit on the board; `None` when not on it.
    pub rank: Option<usize>,
}

pub fn service_record(user: &User, users: &[User], voyages: &[Voyage]) -> ServiceRecord {
    let mine: Vec<&Load> = sold_loads(voyages).filter(|l| took_part(l, user.id)).collect();
    let trade = |pick: fn(f64, f64) -> bool| -> Option<(String, f64)> {
        let mut best: Option<(String, f64)> = None;
        for l in &mine {
            if let Some(p) = l.profit() {
                if best.as_ref().map_or(true, |(_, b)| pick(p, *b)) {
                    best = Some((l.commodity.clone(), p));
                }
            }
        }
        best
    };
    let mut by_commodity: HashMap<&str, usize> = HashMap::new();
    let mut by_route: HashMap<(&str, &str), usize> = HashMap::new();
    for l in &mine {
        *by_commodity.entry(l.commodity.as_str()).or_insert(0) += 1;
        if let Some(to) = l.sell_terminal.as_deref().filter(|_| !l.buy_terminal.is_empty()) {
            *by_route.entry((l.buy_terminal.as_str(), to)).or_insert(0) += 1;
        }
    }
    let favourite_commodity = by_commodity
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(c, n)| (c.to_string(), n));
    let favourite_route = by_route
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|((f, t), n)| (f.to_string(), t.to_string(), n));

    // crates lost on anything this member logged, open or sold
    let logged: Vec<&Load> = voyages
        .iter()
        .flat_map(|v| v.loads.iter())
        .filter(|l| l.submitted_by == user.id)
        .collect();
    let lost: Vec<&Crate> = logged.iter().flat_map(|l| l.crates.iter()).filter(|c| c.is_lost()).collect();

    let mut ships_flown: Vec<String> = voyages
        .iter()
        .filter(|v| v.loads.iter().any(|l| l.submitted_by == user.id) || v.logged_by == user.id)
        .filter_map(|v| v.ship.as_ref().map(|s| s.name.clone()))
        .collect();
    ships_flown.sort();
    ships_flown.dedup();

    let mut dates: Vec<&str> = mine.iter().filter_map(|l| l.sold_at.as_deref()).collect();
    dates.sort_unstable();

    let rank = standings(users, voyages, SortKey::Profit)
        .iter()
        .position(|s| s.user_id == user.id)
        .map(|i| i + 1);

    ServiceRecord {
        user: user.clone(),
        board_credit: board_credit(user.id, voyages),
        cash: cash(user.id, voyages),
        runs_closed: runs_closed_by(user.id, voyages),
        loads_sold: mine.len(),
        scu_hauled: mine.iter().map(|l| l.scu_sold()).sum(),
        best_trade: trade(|p, b| p > b),
        worst_trade: trade(|p, b| p < b),
        favourite_commodity,
        favourite_route,
        crates_lost: lost.len(),
        scu_lost: lost.iter().map(|c| c.size_scu).sum(),
        written_off: logged.iter().map(|l| l.written_off()).sum(),
        // only the seller pays shares out
        shares_paid_out: mine.iter().filter(|l| l.submitted_by == user.id).map(|l| l.shares_paid()).sum(),
        shares_received: sold_loads(voyages)
            .flat_map(|l| l.shares.iter())
            .filter(|s| s.to_user == user.id)
            .map(|s| s.paid_auec)
            .sum(),
        ships_flown,
        first_sale: dates.first().map(|d| d.to_string()),
        latest_sale: dates.last().map(|d| d.to_string()),
        rank,
    }
}

// ---------------------------------------------------------------------------
// crate mixes
// ---------------------------------------------------------------------------

/// `(count, size)` pairs, one per size, largest size first.
pub type BoxMix = Vec<(u32, u32)>;

/// `"2×32, 4x8"` → `[(2, 32), (4, 8)]`. Separators: `,` `;` whitespace;
/// times sign: `x` `X` `×` `*`; sizes must be one of `CRATE_SIZES`; zero
/// counts are refused; repeated sizes are merged; result is normalised
/// (largest first).
pub fn parse_boxes(raw: &str) -> Result<BoxMix, String> {
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    // "2 × 32" with spaces around the sign is one token: glue a bare sign
    // to its neighbours before splitting on whitespace.
    let glued = raw
        .replace(" x ", "x")
        .replace(" X ", "x")
        .replace(" × ", "x")
        .replace(" * ", "x");
    let tokens: Vec<&str> = glued
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Err("Boxes are required -- e.g. 2×32, 4×8.".into());
    }
    for tok in tokens {
        let mut parts = tok.splitn(2, |c: char| matches!(c, 'x' | 'X' | '×' | '*'));
        let (Some(n), Some(size)) = (parts.next(), parts.next()) else {
            return Err(format!("\"{tok}\" isn't count×size -- e.g. 2×32."));
        };
        let n: u32 = n.trim().parse().map_err(|_| format!("\"{tok}\": count must be a whole number."))?;
        let size: u32 = size.trim().parse().map_err(|_| format!("\"{tok}\": size must be a whole number."))?;
        if n == 0 {
            return Err(format!("\"{tok}\": a count of zero is no crate at all."));
        }
        if !CRATE_SIZES.contains(&size) {
            return Err(format!("\"{tok}\": {size} SCU isn't a crate size (1, 2, 4, 8, 16, 24, 32)."));
        }
        *counts.entry(size).or_insert(0) += n;
    }
    Ok(counts.into_iter().rev().map(|(size, n)| (n, size)).collect())
}

/// `[(2, 32), (4, 8)]` → `"2×32, 4×8"`.
pub fn format_boxes(mix: &BoxMix) -> String {
    mix.iter()
        .map(|(n, size)| format!("{n}×{size}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One entry per crate, largest first.
pub fn expand_boxes(mix: &BoxMix) -> Vec<u32> {
    let mut out = Vec::new();
    for &(count, size) in mix {
        for _ in 0..count {
            out.push(size);
        }
    }
    out
}

/// Normalised mix of a bag of sizes.
pub fn mix_of(sizes: impl IntoIterator<Item = u32>) -> BoxMix {
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    for size in sizes {
        *counts.entry(size).or_insert(0) += 1;
    }
    counts.into_iter().rev().map(|(size, n)| (n, size)).collect()
}

// ---------------------------------------------------------------------------
// stowage plan and the sc-cargo.space link
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Deepest,
    MidHold,
    DoorSide,
}

impl Depth {
    pub fn label(&self) -> &'static str {
        match self {
            Depth::Deepest => "Deepest",
            Depth::MidHold => "Mid hold",
            Depth::DoorSide => "Door side",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanEntry {
    pub load_id: i64,
    pub commodity: String,
    pub stop_no: u32,
    pub destination: Option<String>,
    /// Non-lost crate sizes.
    pub sizes: Vec<u32>,
    pub depth: Depth,
}

/// Loads sorted by `stop_no` descending (last stop loads first, deepest);
/// within a stop, larger crates first. First entry `Deepest`, the
/// lowest-stop entries `DoorSide`, anything between `MidHold`.
pub fn stowage_order(loads: &[&Load]) -> Vec<PlanEntry> {
    let mut entries: Vec<PlanEntry> = loads
        .iter()
        .map(|l| {
            let mut sizes: Vec<u32> = l.crates.iter().filter(|c| c.is_live()).map(|c| c.size_scu).collect();
            sizes.sort_unstable_by(|a, b| b.cmp(a));
            PlanEntry {
                load_id: l.id,
                commodity: l.commodity.clone(),
                stop_no: l.stop_no,
                destination: l.destination.clone(),
                sizes,
                depth: Depth::MidHold,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.stop_no
            .cmp(&a.stop_no)
            .then_with(|| b.sizes.first().cmp(&a.sizes.first()))
            .then_with(|| a.load_id.cmp(&b.load_id))
    });
    let min_stop = entries.iter().map(|e| e.stop_no).min().unwrap_or(1);
    for (i, e) in entries.iter_mut().enumerate() {
        e.depth = if i == 0 {
            Depth::Deepest
        } else if e.stop_no == min_stop {
            Depth::DoorSide
        } else {
            Depth::MidHold
        };
    }
    entries
}

/// `https://sc-cargo.space/#/viewer/<slug>-official[-q<n>][-w<n>]…`, sizes
/// `1 2 4 8 16 24 32` = `q w e r t y u`; zero counts omitted; spaces in the
/// slug URL-encoded.
pub fn sc_cargo_link(slug: &str, sizes: &[u32]) -> String {
    let mut url = format!(
        "https://sc-cargo.space/#/viewer/{}-official",
        url_encode(slug)
    );
    for (i, &size) in CRATE_SIZES.iter().enumerate() {
        let n = sizes.iter().filter(|&&s| s == size).count();
        if n > 0 {
            url.push('-');
            url.push(SIZE_KEYS[i]);
            url.push_str(&n.to_string());
        }
    }
    url
}

/// `encodeURIComponent` for the slug: unreserved characters pass, the rest
/// is percent-encoded (a space becomes `%20`).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Hull-less voyages grouped: two join a cluster when any of their loads
/// share a destination (transitively). Undecided destinations never pair.
/// Each cluster is sorted by voyage id; clusters are sorted by their first id.
pub fn cluster_unassigned(voyages: &[&Voyage]) -> Vec<Vec<i64>> {
    let mut un: Vec<&Voyage> = voyages.iter().copied().filter(|v| v.ship.is_none()).collect();
    un.sort_by_key(|v| v.id);
    // union-find over indices
    let mut parent: Vec<usize> = (0..un.len()).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        if p[i] != i {
            let root = find(p, p[i]);
            p[i] = root;
        }
        p[i]
    }
    let mut owner_of_dest: HashMap<String, usize> = HashMap::new();
    for (i, v) in un.iter().enumerate() {
        for d in v.destinations() {
            let key = d.to_lowercase();
            match owner_of_dest.get(&key) {
                Some(&j) => {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    if a != b {
                        parent[a] = b;
                    }
                }
                None => {
                    owner_of_dest.insert(key, i);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for i in 0..un.len() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(un[i].id);
    }
    let mut clusters: Vec<Vec<i64>> = groups.into_values().collect();
    for c in &mut clusters {
        c.sort_unstable();
    }
    clusters.sort_by_key(|c| c[0]);
    clusters
}

// ---------------------------------------------------------------------------
// voyage helpers
// ---------------------------------------------------------------------------

impl Voyage {
    /// `"<owner>'s <ship>"`, or `None` without a hull.
    pub fn hull_label(&self) -> Option<String> {
        self.ship
            .as_ref()
            .map(|s| format!("{}'s {}", self.logged_by_name, s.name))
    }

    pub fn open_loads(&self) -> impl Iterator<Item = &Load> {
        self.loads.iter().filter(|l| l.is_open())
    }

    pub fn is_fully_sold(&self) -> bool {
        !self.loads.is_empty() && self.loads.iter().all(|l| l.is_sold())
    }

    /// Distinct stop numbers across open loads.
    pub fn stop_count(&self) -> usize {
        let mut stops: Vec<u32> = self.open_loads().map(|l| l.stop_no).collect();
        stops.sort_unstable();
        stops.dedup();
        stops.len()
    }

    /// Destinations named on open loads (trimmed, non-empty), deduplicated
    /// case-insensitively, in first-seen order.
    pub fn destinations(&self) -> Vec<String> {
        let mut seen: HashMap<String, ()> = HashMap::new();
        let mut out = Vec::new();
        for l in self.open_loads() {
            if let Some(d) = l.destination.as_deref().map(str::trim) {
                if !d.is_empty() && seen.insert(d.to_lowercase(), ()).is_none() {
                    out.push(d.to_string());
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// dates (unchanged from 0.1)
// ---------------------------------------------------------------------------

/// Today's (UTC) date, the server-side default for date fields; the page
/// swaps in the viewer's local date.
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

// ---------------------------------------------------------------------------
// tests: the specification (RIPAE-ROLL7.md §5 and §6)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub const GIG: i64 = 1;
    pub const JT: i64 = 2;
    pub const THIRD: i64 = 3;

    pub fn users() -> Vec<User> {
        vec![
            User { id: GIG, handle: "lordgig".into(), display_name: "Lord Gig".into(), created_at: "2026-10-21 18:00:00".into(), avatar_url: None },
            User { id: JT, handle: "firstofficer".into(), display_name: "JT".into(), created_at: "2026-10-21 18:00:00".into(), avatar_url: None },
            User { id: THIRD, handle: "crew03".into(), display_name: "Crew-03".into(), created_at: "2026-11-01 09:00:00".into(), avatar_url: None },
        ]
    }

    pub fn name_of(id: i64) -> String {
        match id {
            GIG => "Lord Gig".into(),
            JT => "JT".into(),
            _ => "Crew-03".into(),
        }
    }

    pub fn crate_(id: i64, size: u32, condition: Condition, whereabouts: Whereabouts) -> Crate {
        Crate {
            id,
            load_id: 1,
            size_scu: size,
            condition,
            whereabouts,
            stored_at: (whereabouts == Whereabouts::Stored).then(|| "Everus Harbor".to_string()),
            sold_at: (condition == Condition::Sold).then(|| "2026-11-08".to_string()),
        }
    }

    pub fn sound(id: i64, size: u32) -> Crate {
        crate_(id, size, Condition::Sound, Whereabouts::Aboard)
    }

    pub fn load(id: i64, buy_price_scu: f64, crates: Vec<Crate>) -> Load {
        Load {
            id,
            voyage_id: 1,
            submitted_by: GIG,
            submitted_by_name: name_of(GIG),
            bought_by: GIG,
            bought_by_name: name_of(GIG),
            commodity: "Laranite".into(),
            kind: LoadKind::Trade,
            buy_terminal: "ArcCorp Mining Area 141".into(),
            price_mode: PriceMode::Scu,
            buy_price_scu,
            buy_price_lot: None,
            stop_no: 1,
            destination: Some("Area18 TDD".into()),
            sold_at: None,
            sell_terminal: None,
            sell_price_scu: None,
            sell_price_lot: None,
            reward: None,
            notes: None,
            crates: crates
                .into_iter()
                .map(|mut c| {
                    c.load_id = id;
                    c
                })
                .collect(),
            shares: vec![],
        }
    }

    /// A load sold whole by `seller` at `sell_price_scu`, with `shares`
    /// already settled the way the repo would store them.
    pub fn sold_load(
        id: i64,
        seller: i64,
        buy: f64,
        sell: f64,
        sizes: &[u32],
        shares: Vec<(i64, Option<i64>, ShareKind, f64)>,
    ) -> Load {
        let crates: Vec<Crate> = sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| crate_(id * 100 + i as i64, s, Condition::Sold, Whereabouts::Aboard))
            .collect();
        let mut l = load(id, buy, crates);
        l.submitted_by = seller;
        l.submitted_by_name = name_of(seller);
        let scu: u32 = sizes.iter().sum();
        l.sold_at = Some("2026-11-08".into());
        l.sell_terminal = Some("Area18 TDD".into());
        l.sell_price_scu = Some(sell);
        l.sell_price_lot = Some(sell * scu as f64);
        let profit = (sell - buy) * scu as f64;
        l.shares = shares
            .into_iter()
            .enumerate()
            .map(|(i, (to, paid_to, kind, amount))| Share {
                id: id * 10 + i as i64,
                load_id: id,
                to_user: to,
                to_user_name: name_of(to),
                paid_to: paid_to.unwrap_or(to),
                paid_to_name: name_of(paid_to.unwrap_or(to)),
                kind,
                amount,
                reason: None,
                paid_auec: share_paid(profit, kind, amount),
            })
            .collect();
        l
    }

    pub fn ship(name: &str, slug: &str) -> Ship {
        Ship {
            id: 1,
            manufacturer: "MISC".into(),
            name: name.into(),
            short: name.into(),
            sc_cargo_slug: slug.into(),
            capacity_scu: 4608,
            concept: false,
        }
    }

    pub fn voyage(id: i64, owner: i64, ship: Option<Ship>, loads: Vec<Load>) -> Voyage {
        Voyage {
            id,
            logged_by: owner,
            logged_by_name: name_of(owner),
            entered_by: owner,
            entered_by_name: name_of(owner),
            ship,
            bought_at: "2026-11-08".into(),
            notes: None,
            loads: loads
                .into_iter()
                .map(|mut l| {
                    l.voyage_id = id;
                    l
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
    }

    // -- derived status ----------------------------------------------------

    #[test]
    fn load_status_all_aboard_is_in_transit() {
        let l = load(1, 10.0, vec![sound(1, 32), sound(2, 8)]);
        assert_eq!(l.status(), LoadStatus::InTransit);
        assert!(l.is_open());
    }

    #[test]
    fn load_status_all_stored_is_stored() {
        let l = load(
            1,
            10.0,
            vec![
                crate_(1, 32, Condition::Sound, Whereabouts::Stored),
                crate_(2, 8, Condition::Glitched, Whereabouts::Stored),
            ],
        );
        assert_eq!(l.status(), LoadStatus::Stored);
    }

    #[test]
    fn load_status_all_pickup_is_awaiting() {
        let l = load(
            1,
            10.0,
            vec![crate_(1, 8, Condition::Sound, Whereabouts::AwaitingPickup)],
        );
        assert_eq!(l.status(), LoadStatus::AwaitingPickup);
    }

    #[test]
    fn load_status_mixed_is_split() {
        let l = load(
            1,
            10.0,
            vec![
                sound(1, 32),
                crate_(2, 32, Condition::Sound, Whereabouts::Stored),
                // a lost crate's whereabouts don't count
                crate_(3, 8, Condition::Lost, Whereabouts::AwaitingPickup),
            ],
        );
        assert_eq!(l.status(), LoadStatus::Split);
    }

    #[test]
    fn load_status_all_sold_or_lost_is_sold() {
        let l = load(
            1,
            10.0,
            vec![
                crate_(1, 32, Condition::Sold, Whereabouts::Aboard),
                crate_(2, 8, Condition::Lost, Whereabouts::Aboard),
            ],
        );
        assert_eq!(l.status(), LoadStatus::Sold);
        assert!(l.is_sold());
        // lost-only is "sold" too: nothing left to sell
        let gone = load(2, 10.0, vec![crate_(1, 8, Condition::Lost, Whereabouts::Aboard)]);
        assert_eq!(gone.status(), LoadStatus::Sold);
    }

    #[test]
    fn where_now_lists_only_nonzero_groups() {
        let l = load(
            1,
            10.0,
            vec![
                sound(1, 32),
                crate_(2, 8, Condition::Sound, Whereabouts::Stored),
                crate_(3, 8, Condition::Glitched, Whereabouts::Stored),
                crate_(4, 8, Condition::Lost, Whereabouts::AwaitingPickup),
            ],
        );
        assert_eq!(
            l.where_now(Some("JT's Hull C")),
            "1 aboard JT's Hull C · 2 stored at Everus Harbor"
        );
        // a single group drops the count
        let aboard = load(2, 10.0, vec![sound(1, 32), sound(2, 32)]);
        assert_eq!(aboard.where_now(Some("JT's Hull C")), "aboard JT's Hull C");
        assert_eq!(aboard.where_now(None), "aboard · no hull yet");
        let stored = load(3, 10.0, vec![crate_(1, 8, Condition::Sound, Whereabouts::Stored)]);
        assert_eq!(stored.where_now(None), "at Everus Harbor");
        let pickup = load(4, 10.0, vec![crate_(1, 8, Condition::Sound, Whereabouts::AwaitingPickup)]);
        assert_eq!(pickup.where_now(None), "awaiting pickup at ArcCorp Mining Area 141");
        // free cargo may have no buy terminal
        let mut free = load(5, 0.0, vec![crate_(1, 8, Condition::Sound, Whereabouts::AwaitingPickup)]);
        free.buy_terminal = String::new();
        assert_eq!(free.where_now(None), "awaiting pickup");
    }

    #[test]
    fn a_load_without_a_buy_terminal_is_no_route() {
        let mut free = sold_load(1, GIG, 0.0, 30.0, &[32], vec![]);
        free.buy_terminal = String::new();
        let vs = vec![voyage(1, GIG, None, vec![free])];
        let rows = standings(&users(), &vs, SortKey::Profit);
        let gig = rows.iter().find(|r| r.display_name == "Lord Gig").unwrap();
        assert_eq!(gig.loads_sold, 1);
        assert!(gig.best_route.is_none());
        assert!(service_record(&users()[0], &users(), &vs).favourite_route.is_none());
    }

    // -- money -------------------------------------------------------------

    #[test]
    fn scu_excludes_lost_crates() {
        let l = load(
            1,
            10.0,
            vec![sound(1, 32), sound(2, 8), crate_(3, 8, Condition::Lost, Whereabouts::Aboard)],
        );
        assert_eq!(l.scu(), 40);
        assert_eq!(l.scu_all(), 48);
        assert_eq!(l.scu_lost(), 8);
    }

    #[test]
    fn cost_includes_lost_crates() {
        let l = load(
            1,
            10.0,
            vec![sound(1, 32), crate_(2, 8, Condition::Lost, Whereabouts::Aboard)],
        );
        assert_eq!(l.cost(), 400.0);
    }

    #[test]
    fn written_off_is_lost_scu_times_buy_price() {
        let l = load(
            1,
            12.5,
            vec![sound(1, 32), crate_(2, 8, Condition::Lost, Whereabouts::Aboard)],
        );
        assert_eq!(l.written_off(), 100.0);
        assert_eq!(load(2, 12.5, vec![sound(1, 32)]).written_off(), 0.0);
    }

    #[test]
    fn per_lot_buy_derives_per_scu() {
        assert_eq!(derive_prices(PriceMode::Lot, 1000.0, 100), (10.0, 1000.0));
        // no crates: nothing to divide by, per-SCU is 0 rather than NaN
        assert_eq!(derive_prices(PriceMode::Lot, 1000.0, 0), (0.0, 1000.0));
    }

    #[test]
    fn per_scu_buy_derives_per_lot() {
        assert_eq!(derive_prices(PriceMode::Scu, 10.0, 100), (10.0, 1000.0));
    }

    #[test]
    fn per_lot_sell_derives_per_scu() {
        let l = load(1, 10.0, vec![sound(1, 32), sound(2, 8)]);
        let sale = settle(
            &l,
            &SaleInput { crate_ids: vec![1, 2], price_mode: PriceMode::Lot, price: 600.0, shares: vec![] },
        )
        .unwrap();
        assert_eq!(sale.scu_sold, 40);
        assert_eq!(sale.sell_price_lot, 600.0);
        assert_eq!(sale.sell_price_scu, 15.0);
        assert_eq!(sale.revenue, 600.0);
        assert_eq!(sale.profit, 200.0);
    }

    #[test]
    fn partial_close_splits_load_and_keeps_rest_open() {
        let l = load(
            1,
            10.0,
            vec![
                sound(1, 32),
                sound(2, 8),
                crate_(3, 8, Condition::Glitched, Whereabouts::Aboard),
                crate_(4, 8, Condition::Lost, Whereabouts::Aboard),
            ],
        );
        let sale = settle(
            &l,
            &SaleInput { crate_ids: vec![1], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] },
        )
        .unwrap();
        assert_eq!(sale.sold.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1]);
        // kept: the other sound crate, the glitched one, and the lost one
        assert_eq!(sale.kept.iter().map(|c| c.id).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert!(!sale.closes_load());

        let mut rest = l.clone();
        rest.crates = sale.kept.clone();
        assert!(rest.is_open());
        assert_eq!(rest.scu(), 16);

        // selling everything sellable closes the load; the lost crate rides along
        let all = settle(
            &l,
            &SaleInput { crate_ids: vec![1, 2, 3], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] },
        )
        .unwrap();
        assert!(all.closes_load());
        assert_eq!(all.kept.iter().map(|c| c.id).collect::<Vec<_>>(), vec![4]);
    }

    #[test]
    fn profit_uses_cost_of_crates_sold_only() {
        let l = load(1, 10.0, vec![sound(1, 32), sound(2, 8), sound(3, 8)]);
        let sale = settle(
            &l,
            &SaleInput { crate_ids: vec![1], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] },
        )
        .unwrap();
        assert_eq!(sale.cost_sold, 320.0);
        assert_eq!(sale.revenue, 480.0);
        assert_eq!(sale.profit, 160.0);
    }

    #[test]
    fn pct_share_on_loss_is_zero() {
        assert_eq!(share_paid(-500.0, ShareKind::Pct, 25.0), 0.0);
        assert_eq!(share_paid(0.0, ShareKind::Pct, 25.0), 0.0);
    }

    #[test]
    fn flat_share_on_loss_is_zero() {
        // ruled 8 Nov: nothing is owed on a losing run, flat fees included
        assert_eq!(share_paid(-500.0, ShareKind::Flat, 50_000.0), 0.0);
        assert_eq!(share_paid(0.0, ShareKind::Flat, 50_000.0), 0.0);
    }

    #[test]
    fn flat_share_on_profit_is_full_amount() {
        assert_eq!(share_paid(1000.0, ShareKind::Flat, 250.0), 250.0);
        assert_eq!(share_paid(1000.0, ShareKind::Pct, 25.0), 250.0);
    }

    #[test]
    fn shares_sum_plus_kept_equals_profit() {
        let l = load(1, 10.0, vec![sound(1, 32), sound(2, 32)]); // 64 SCU
        let sale = settle(
            &l,
            &SaleInput {
                crate_ids: vec![1, 2],
                price_mode: PriceMode::Scu,
                price: 20.0, // profit 640
                shares: vec![
                    ShareInput { to_user: JT, paid_to: None, kind: ShareKind::Pct, amount: 25.0, reason: None },
                    ShareInput { to_user: THIRD, paid_to: None, kind: ShareKind::Flat, amount: 100.0, reason: Some("escort".into()) },
                ],
            },
        )
        .unwrap();
        assert_eq!(sale.profit, 640.0);
        assert_eq!(sale.shares[0].paid, 160.0);
        assert_eq!(sale.shares[1].paid, 100.0);
        assert_eq!(sale.seller_keeps, 380.0);
        let paid: f64 = sale.shares.iter().map(|s| s.paid).sum();
        assert!(approx(paid + sale.seller_keeps, sale.profit));
    }

    #[test]
    fn settle_refuses_lost_and_unknown_crates() {
        let l = load(1, 10.0, vec![sound(1, 32), crate_(2, 8, Condition::Lost, Whereabouts::Aboard)]);
        let lost = settle(&l, &SaleInput { crate_ids: vec![1, 2], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] });
        assert!(lost.is_err(), "lost crates can't be sold");
        let unknown = settle(&l, &SaleInput { crate_ids: vec![99], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] });
        assert!(unknown.is_err());
        let none = settle(&l, &SaleInput { crate_ids: vec![], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] });
        assert!(none.is_err(), "a sale needs at least one crate");
    }

    #[test]
    fn forfeit_moves_cash_not_credit() {
        // Gig sells (profit 1000) and gives JT 25 %; JT forfeits the aUEC to Crew-03.
        let v = voyage(
            1,
            GIG,
            None,
            vec![sold_load(1, GIG, 10.0, 20.0, &[32, 32, 32, 4], vec![(JT, Some(THIRD), ShareKind::Pct, 25.0)])],
        );
        let vs = vec![v];
        assert_eq!(board_credit(GIG, &vs), 750.0);
        assert_eq!(board_credit(JT, &vs), 250.0);
        assert_eq!(board_credit(THIRD, &vs), 0.0);
        // Gig took in 2000 and paid 250 of it out; it landed with Crew-03
        assert_eq!(cash(GIG, &vs), 1750.0);
        assert_eq!(cash(JT, &vs), 0.0);
        assert_eq!(cash(THIRD, &vs), 250.0);
    }

    #[test]
    fn credit_sums_to_profit_and_cash_to_revenue() {
        let vs = vec![
            voyage(1, GIG, None, vec![sold_load(1, GIG, 10.0, 20.0, &[32, 32], vec![(JT, Some(THIRD), ShareKind::Pct, 30.0)])]),
            voyage(2, JT, None, vec![sold_load(2, JT, 10.0, 5.0, &[8], vec![(GIG, None, ShareKind::Flat, 1000.0)])]),
            voyage(3, JT, None, vec![sold_load(3, JT, 1.0, 3.0, &[16, 8], vec![(GIG, None, ShareKind::Flat, 10.0), (THIRD, Some(GIG), ShareKind::Pct, 10.0)])]),
        ];
        let credit: f64 = [GIG, JT, THIRD].iter().map(|&u| board_credit(u, &vs)).sum();
        let money: f64 = [GIG, JT, THIRD].iter().map(|&u| cash(u, &vs)).sum();
        let realised: f64 = vs.iter().flat_map(|v| v.loads.iter()).filter_map(|l| l.profit()).sum();
        assert!(approx(credit, realised));
        // shares only move takings between members: the org collected every sale
        let revenue: f64 = vs.iter().flat_map(|v| v.loads.iter()).filter_map(|l| l.revenue()).sum();
        assert!(approx(money, revenue), "{money} vs {revenue}");
    }

    #[test]
    fn committed_subtracts_written_off() {
        let open = load(
            1,
            10.0,
            vec![sound(1, 32), sound(2, 32), crate_(3, 8, Condition::Lost, Whereabouts::Aboard)],
        );
        let sold = sold_load(2, GIG, 10.0, 15.0, &[8], vec![]);
        let vs = vec![voyage(1, GIG, None, vec![open]), voyage(2, GIG, None, vec![sold])];
        let t = Totals::from_voyages(&vs);
        assert_eq!(t.committed_gross, 720.0);
        assert_eq!(t.written_off, 80.0);
        assert_eq!(t.committed, 640.0);
        assert_eq!(t.open_voyages, 1);
        assert_eq!(t.open_loads, 1);
        assert_eq!(t.open_scu, 64);
    }

    #[test]
    fn realised_counts_sold_crates_only() {
        let open = load(1, 10.0, vec![sound(1, 32)]);
        let sold = sold_load(2, GIG, 10.0, 15.0, &[8, 8], vec![(JT, None, ShareKind::Pct, 50.0)]);
        let vs = vec![voyage(1, GIG, None, vec![open, sold])];
        let t = Totals::from_voyages(&vs);
        // profit (15-10)×16 = 80, shares don't change what the org realised
        assert_eq!(t.realised, 80.0);
        assert_eq!(t.committed, 320.0);
    }

    #[test]
    fn realised_is_net_of_every_write_off() {
        // open: 8 SCU lost at 10 → 80
        let open = load(1, 10.0, vec![sound(1, 32), crate_(2, 8, Condition::Lost, Whereabouts::Aboard)]);
        // sold: profit (15-10)×16 = 80, plus a lost 8 SCU crate at 10 → 80
        let mut sold = sold_load(2, GIG, 10.0, 15.0, &[8, 8], vec![]);
        sold.crates.push(crate_(3, 8, Condition::Lost, Whereabouts::Aboard));
        // written off whole, never sold: 4 SCU at 25 → 100
        let gone = load(3, 25.0, vec![crate_(4, 4, Condition::Lost, Whereabouts::Aboard)]);
        // voyage 2 is closed (nothing open), so its write-offs are off realised
        let vs = vec![voyage(1, GIG, None, vec![open]), voyage(2, GIG, None, vec![sold, gone])];
        let t = Totals::from_voyages(&vs);
        assert_eq!(t.losses, 180.0);
        assert_eq!(t.realised, 80.0 - 180.0);
        // committed is still what's afloat, write-offs already off it
        assert_eq!(t.written_off, 80.0);
        assert_eq!(t.committed, 320.0);
    }

    #[test]
    fn write_offs_come_off_the_owner_not_the_share_holders() {
        // JT on 50 % of a +160 sale; Gig also lost 8 SCU at 10 on an open load
        let sold = sold_load(1, GIG, 10.0, 20.0, &[16], vec![(JT, None, ShareKind::Pct, 50.0)]);
        let gone = load(2, 10.0, vec![crate_(2, 8, Condition::Lost, Whereabouts::Aboard)]);
        let vs = vec![voyage(1, GIG, None, vec![sold, gone])];
        assert_eq!(losses(GIG, &vs), 80.0);
        assert_eq!(board_credit(GIG, &vs), 80.0 - 80.0);
        // took in 320, paid JT 80; the lost crate cost money, it wasn't taken
        assert_eq!(cash(GIG, &vs), 320.0 - 80.0);
        assert_eq!(board_credit(JT, &vs), 80.0);
        let rec = service_record(&users()[0], &users(), &vs);
        assert_eq!(rec.written_off, 80.0);
        assert_eq!(rec.board_credit, 0.0);
    }

    #[test]
    fn open_voyage_write_off_waits_until_the_voyage_closes() {
        // one voyage: a +80 sale, plus an open load with 8 SCU lost at 10
        let sold = sold_load(1, GIG, 10.0, 15.0, &[16], vec![]);
        let open = load(2, 10.0, vec![sound(1, 8), crate_(2, 8, Condition::Lost, Whereabouts::Aboard)]);
        let mut vs = vec![voyage(1, GIG, None, vec![sold, open])];
        let t = Totals::from_voyages(&vs);
        assert_eq!((t.losses, t.realised), (0.0, 80.0));
        // still off what's afloat
        assert_eq!((t.written_off, t.committed), (80.0, 80.0));
        assert_eq!(losses(GIG, &vs), 0.0);
        assert_eq!(board_credit(GIG, &vs), 80.0);
        assert_eq!(cash(GIG, &vs), 240.0);
        // the record still shows the lost crate
        assert_eq!(service_record(&users()[0], &users(), &vs).written_off, 80.0);

        // sell the sound crate at cost: nothing open left, the write-off lands
        let l = &mut vs[0].loads[1];
        l.crates[0].condition = Condition::Sold;
        l.sell_price_scu = Some(10.0);
        l.sold_at = Some("2955-01-02".into());
        assert!(vs[0].is_fully_sold());
        let t = Totals::from_voyages(&vs);
        assert_eq!((t.losses, t.realised, t.committed), (80.0, 0.0, 0.0));
        assert_eq!(board_credit(GIG, &vs), 0.0);
        // the write-off is off profit, not off what the two sales took in
        assert_eq!(cash(GIG, &vs), 240.0 + 80.0);
    }

    // -- standings -----------------------------------------------------------

    #[test]
    fn standings_rank_by_net_profit_and_count_runs_loads_scu() {
        let vs = vec![
            // Gig's voyage, fully sold: two loads, one with a share to JT
            voyage(1, GIG, None, vec![
                sold_load(1, GIG, 10.0, 20.0, &[32, 32], vec![(JT, None, ShareKind::Pct, 25.0)]), // profit 640, keeps 480
                sold_load(2, GIG, 10.0, 12.0, &[8], vec![]),                                      // profit 16
            ]),
            // JT's voyage, one load sold, one still open -> not a closed run
            voyage(2, JT, None, vec![
                sold_load(3, JT, 10.0, 30.0, &[32], vec![]), // profit 640
                load(4, 10.0, vec![sound(1, 8)]),
            ]),
        ];
        let rows = standings(&users(), &vs, SortKey::Profit);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].display_name, "JT");
        assert_eq!(rows[0].board_credit, 800.0); // 640 sold + 160 share
        // JT's share of Gig's load 1 counts: Gig's closed run, that load, its 64 SCU
        assert_eq!(rows[0].runs_closed, 1);
        assert_eq!(rows[0].loads_sold, 2);
        assert_eq!(rows[0].scu_hauled, 96);
        assert_eq!(rows[1].display_name, "Lord Gig");
        assert_eq!(rows[1].board_credit, 496.0);
        assert_eq!(rows[1].runs_closed, 1);
        assert_eq!(rows[1].loads_sold, 2);
        assert_eq!(rows[1].scu_hauled, 72);
        assert_eq!(rows[1].best_route.as_ref().map(|r| r.profit), Some(640.0));
        assert_eq!(rows[2].display_name, "Crew-03");
        assert_eq!(rows[2].board_credit, 0.0);
        let pct: f64 = rows.iter().map(|r| r.share_of_org).sum();
        assert!(approx(pct, 100.0));

        // one closed run each: the tie goes to net profit
        let by_runs = standings(&users(), &vs, SortKey::Runs);
        assert_eq!((by_runs[0].display_name.as_str(), by_runs[0].runs_closed), ("JT", 1));
        assert_eq!((by_runs[1].display_name.as_str(), by_runs[1].runs_closed), ("Lord Gig", 1));
        let by_scu = standings(&users(), &vs, SortKey::Scu);
        assert_eq!(by_scu.iter().map(|r| r.scu_hauled).collect::<Vec<_>>(), vec![96, 72, 0]);
    }

    #[test]
    fn resettle_reprices_a_sale_and_pays_shares_on_the_new_profit() {
        // 16 SCU bought at 10, recorded at 15/SCU; really 3,000 for the lot
        let l = sold_load(1, GIG, 10.0, 15.0, &[8, 8], vec![(JT, None, ShareKind::Pct, 50.0)]);
        let half = ShareInput { to_user: JT, paid_to: None, kind: ShareKind::Pct, amount: 50.0, reason: None };
        let r = resettle(&l, PriceMode::Lot, 3000.0, &[half.clone()]).unwrap();
        assert_eq!((r.sell_price_scu, r.sell_price_lot), (187.5, 3000.0));
        assert_eq!(r.profit, 2840.0);
        assert_eq!(r.shares[0].paid, 1420.0);
        // a loss pays nothing, flat or percent
        let flat = ShareInput { kind: ShareKind::Flat, amount: 100.0, ..half.clone() };
        let r = resettle(&l, PriceMode::Scu, 5.0, &[flat]).unwrap();
        assert_eq!(r.profit, -80.0);
        assert_eq!(r.shares[0].paid, 0.0);
        // over 100 % refused; an unsold load can't be re-priced
        let over = ShareInput { amount: 60.0, ..half.clone() };
        assert!(resettle(&l, PriceMode::Lot, 3000.0, &[over.clone(), over]).is_err());
        assert!(resettle(&load(2, 10.0, vec![sound(1, 8)]), PriceMode::Lot, 1.0, &[]).is_err());
    }

    #[test]
    fn a_half_share_member_who_never_sold_still_flew_the_runs() {
        // JT sells both routes; Gig holds 50 % of each
        let vs = vec![
            voyage(1, JT, None, vec![sold_load(1, JT, 10.0, 20.0, &[32], vec![(GIG, None, ShareKind::Pct, 50.0)])]),
            voyage(2, JT, None, vec![sold_load(2, JT, 10.0, 15.0, &[8], vec![(GIG, None, ShareKind::Pct, 50.0)])]),
        ];
        let rows = standings(&users(), &vs, SortKey::Profit);
        let gig = rows.iter().find(|r| r.user_id == GIG).unwrap();
        assert_eq!((gig.runs_closed, gig.loads_sold, gig.scu_hauled), (2, 2, 40));
        assert_eq!(gig.board_credit, 180.0); // half of 320 + half of 40
        assert!(gig.best_route.is_some());
        let rec = service_record(&users()[0], &users(), &vs);
        assert_eq!(rec.user.id, GIG);
        assert_eq!((rec.runs_closed, rec.loads_sold), (2, 2));
        assert_eq!(rec.shares_paid_out, 0.0, "Gig paid nobody; JT paid Gig");
        assert_eq!(rec.shares_received, 180.0);
    }

    /// Gig holds 100 % of JT's +640 sale and forfeits it to JT; Gig also
    /// sells +80 of his own; Gig has a third, unsold voyage.
    fn forfeit_ledger() -> Vec<Voyage> {
        vec![
            voyage(1, JT, None, vec![sold_load(1, JT, 10.0, 30.0, &[32], vec![(GIG, Some(JT), ShareKind::Pct, 100.0)])]),
            voyage(2, GIG, None, vec![sold_load(2, GIG, 10.0, 20.0, &[8], vec![])]),
            voyage(3, GIG, None, vec![load(3, 10.0, vec![sound(1, 8)])]),
        ]
    }

    fn row_of(rows: &[Standing], id: i64) -> &Standing {
        rows.iter().find(|r| r.user_id == id).unwrap()
    }

    #[test]
    fn profit_counts_a_forfeited_share_and_collected_does_not() {
        let vs = forfeit_ledger();
        let rows = standings(&users(), &vs, SortKey::Profit);
        let (gig, jt) = (row_of(&rows, GIG), row_of(&rows, JT));
        // Gig: the 160 Gig sold for; JT: took 960, "paid" Gig 640 that came back to JT
        assert_eq!((gig.board_credit, gig.cash, gig.forfeited, gig.forfeits_received), (720.0, 160.0, 640.0, 0.0));
        assert_eq!((jt.board_credit, jt.cash, jt.forfeited, jt.forfeits_received), (0.0, 960.0, 0.0, 640.0));
        // profit: Gig has all of it; collected: JT holds most of the cash
        assert_eq!(rows[0].user_id, GIG);
        assert!(approx(rows[0].share_of_org, 100.0));
        let by_cash = standings(&users(), &vs, SortKey::Collected);
        assert_eq!(by_cash.iter().map(|r| r.user_id).collect::<Vec<_>>(), vec![JT, GIG, THIRD]);
        assert!(approx(by_cash[0].share_of_org, 960.0 / 1120.0 * 100.0));
        assert!(approx(by_cash.iter().map(|r| r.share_of_org).sum::<f64>(), 100.0));
    }

    #[test]
    fn the_share_bar_follows_the_sort() {
        let vs = forfeit_ledger();
        // two closed runs (voyage 3 is still open); Gig flew both, JT one
        let by_runs = standings(&users(), &vs, SortKey::Runs);
        assert_eq!(by_runs.iter().map(|r| (r.user_id, r.runs_closed)).collect::<Vec<_>>(), vec![(GIG, 2), (JT, 1), (THIRD, 0)]);
        assert_eq!(by_runs.iter().map(|r| r.share_of_org).collect::<Vec<_>>(), vec![100.0, 50.0, 0.0]);
        // 40 SCU sold; Gig was on all of it, JT on his 32
        let by_scu = standings(&users(), &vs, SortKey::Scu);
        assert_eq!(by_scu.iter().map(|r| (r.user_id, r.scu_hauled)).collect::<Vec<_>>(), vec![(GIG, 40), (JT, 32), (THIRD, 0)]);
        assert_eq!(by_scu.iter().map(|r| r.share_of_org).collect::<Vec<_>>(), vec![100.0, 80.0, 0.0]);
        for key in SortKey::ALL {
            assert!(standings(&users(), &vs, key).iter().all(|r| (0.0..=100.0).contains(&r.share_of_org)), "{key:?}");
        }
    }

    #[test]
    fn a_loss_takes_no_share_and_an_empty_board_is_all_zero() {
        // Gig loses 80 on his only sale: no negative share, no NaN
        let vs = vec![voyage(1, GIG, None, vec![sold_load(1, GIG, 20.0, 10.0, &[8], vec![])])];
        let rows = standings(&users(), &vs, SortKey::Profit);
        assert!(rows.iter().all(|r| r.share_of_org == 0.0));
        assert_eq!(rows.last().unwrap().user_id, GIG, "the loss sorts last");
        // the 80 it sold for still came in
        let by_cash = standings(&users(), &vs, SortKey::Collected);
        assert_eq!((by_cash[0].user_id, by_cash[0].cash, by_cash[0].share_of_org), (GIG, 80.0, 100.0));
        for key in SortKey::ALL {
            let rows = standings(&users(), &[], key);
            assert!(rows.iter().all(|r| r.share_of_org == 0.0 && r.is_idle()));
            // everything tied: A→Z, ignoring case
            assert_eq!(rows.iter().map(|r| r.display_name.as_str()).collect::<Vec<_>>(), vec!["Crew-03", "JT", "Lord Gig"]);
        }
    }

    #[test]
    fn ties_on_the_sort_key_fall_back_to_profit_then_collected() {
        // both flew both runs; the tie on runs goes to profit
        let vs = vec![
            voyage(1, GIG, None, vec![sold_load(1, GIG, 10.0, 20.0, &[16], vec![(JT, Some(GIG), ShareKind::Pct, 50.0)])]),
            voyage(2, JT, None, vec![sold_load(2, JT, 10.0, 20.0, &[16], vec![(GIG, Some(JT), ShareKind::Pct, 25.0)])]),
        ];
        let rows = standings(&users(), &vs, SortKey::Runs);
        // Gig: 80 kept + 40 earned = 120 credit, 320 taken − 80 paid + 80 forfeited back = 320 cash;
        // JT: 120 kept + 80 earned = 200 credit, 320 − 40 + 40 = 320 cash
        assert_eq!((row_of(&rows, GIG).board_credit, row_of(&rows, GIG).cash), (120.0, 320.0));
        assert_eq!((row_of(&rows, JT).board_credit, row_of(&rows, JT).cash), (200.0, 320.0));
        assert_eq!(rows.iter().map(|r| r.user_id).collect::<Vec<_>>(), vec![JT, GIG, THIRD]);
        // equal cash: the tie on collected goes to profit
        let by_cash = standings(&users(), &vs, SortKey::Collected);
        assert_eq!(by_cash.iter().map(|r| r.user_id).collect::<Vec<_>>(), vec![JT, GIG, THIRD]);
    }

    #[test]
    fn sort_keys_parse_forgivingly() {
        assert_eq!(SortKey::parse("collected"), SortKey::Collected);
        assert_eq!(SortKey::parse("cash"), SortKey::Collected);
        assert_eq!(SortKey::parse(" SCU "), SortKey::Scu);
        assert_eq!(SortKey::parse("Runs"), SortKey::Runs);
        assert_eq!(SortKey::parse("nonsense"), SortKey::Profit);
        for k in SortKey::ALL {
            assert_eq!(SortKey::parse(k.as_str()), k);
        }
    }

    #[test]
    fn service_record_reads_best_worst_favourites_and_rank() {
        let vs = vec![
            voyage(1, GIG, Some(ship("Hull-C", "hull-c")), vec![
                sold_load(1, GIG, 10.0, 20.0, &[32], vec![(JT, None, ShareKind::Pct, 25.0)]),
                sold_load(2, GIG, 10.0, 5.0, &[8], vec![]),
            ]),
            voyage(2, JT, None, vec![sold_load(3, JT, 10.0, 30.0, &[32], vec![])]),
        ];
        let us = users();
        let rec = service_record(&us[0], &us, &vs);
        assert_eq!(rec.loads_sold, 2);
        assert_eq!(rec.runs_closed, 1);
        assert_eq!(rec.best_trade, Some(("Laranite".to_string(), 320.0)));
        assert_eq!(rec.worst_trade, Some(("Laranite".to_string(), -40.0)));
        assert_eq!(rec.favourite_commodity, Some(("Laranite".to_string(), 2)));
        assert_eq!(rec.shares_paid_out, 80.0);
        assert_eq!(rec.ships_flown, vec!["Hull-C".to_string()]);
        assert_eq!(rec.rank, Some(2)); // JT's 640 beats Gig's 240+(-40)=200
        let jt = service_record(&us[1], &us, &vs);
        assert_eq!(jt.rank, Some(1));
        assert_eq!(jt.shares_received, 80.0);
    }

    // -- crate mixes ---------------------------------------------------------

    #[test]
    fn parse_boxes_accepts_commas_spaces_and_three_times_signs() {
        assert_eq!(parse_boxes("2×32, 4x8").unwrap(), vec![(2, 32), (4, 8)]);
        assert_eq!(parse_boxes("2X32 4*8;1x1").unwrap(), vec![(2, 32), (4, 8), (1, 1)]);
        assert_eq!(parse_boxes("  4x8 , 2 × 32 ").unwrap(), vec![(2, 32), (4, 8)]);
        // repeated sizes merge
        assert_eq!(parse_boxes("1x32, 2x32").unwrap(), vec![(3, 32)]);
        assert_eq!(format_boxes(&vec![(2, 32), (4, 8)]), "2×32, 4×8");
        assert_eq!(expand_boxes(&vec![(2, 32), (1, 8)]), vec![32, 32, 8]);
    }

    #[test]
    fn parse_boxes_rejects_bad_sizes_and_zero_counts() {
        assert!(parse_boxes("2x12").is_err());
        assert!(parse_boxes("0x32").is_err());
        assert!(parse_boxes("").is_err());
        assert!(parse_boxes("32").is_err());
        assert!(parse_boxes("twox32").is_err());
        assert!(parse_boxes("2x32, junk").is_err());
    }

    // -- stowage plan --------------------------------------------------------

    #[test]
    fn stowage_order_last_stop_first_then_larger_crates() {
        let mut a = load(1, 1.0, vec![sound(1, 32)]); // stop 1
        a.commodity = "Quantainium".into();
        let mut b = load(2, 1.0, vec![sound(2, 32), sound(3, 32)]); // stop 2
        b.stop_no = 2;
        b.commodity = "Agricium".into();
        let mut c = load(3, 1.0, vec![sound(4, 8), sound(5, 8), sound(6, 8), sound(7, 8), sound(8, 8)]); // stop 1, 40 SCU of small boxes
        c.commodity = "Aluminum".into();
        let order = stowage_order(&[&a, &b, &c]);
        assert_eq!(order.iter().map(|e| e.load_id).collect::<Vec<_>>(), vec![2, 1, 3]);
        assert_eq!(order[0].stop_no, 2);
    }

    #[test]
    fn stowage_labels_deepest_doorside_midhold() {
        let mk = |id: i64, stop: u32, size: u32| {
            let mut l = load(id, 1.0, vec![sound(id * 10, size)]);
            l.stop_no = stop;
            l
        };
        let (a, b, c, d) = (mk(1, 3, 32), mk(2, 2, 32), mk(3, 1, 32), mk(4, 1, 8));
        let order = stowage_order(&[&a, &b, &c, &d]);
        assert_eq!(
            order.iter().map(|e| e.depth).collect::<Vec<_>>(),
            vec![Depth::Deepest, Depth::MidHold, Depth::DoorSide, Depth::DoorSide]
        );
        // one stop: first is deepest, the rest door side
        let (e, f) = (mk(5, 1, 32), mk(6, 1, 8));
        let order = stowage_order(&[&e, &f]);
        assert_eq!(order.iter().map(|x| x.depth).collect::<Vec<_>>(), vec![Depth::Deepest, Depth::DoorSide]);
        // lost crates don't ship
        let mut g = mk(7, 1, 32);
        g.crates.push(crate_(71, 8, Condition::Lost, Whereabouts::Aboard));
        assert_eq!(stowage_order(&[&g])[0].sizes, vec![32]);
    }

    #[test]
    fn unassigned_voyages_cluster_by_shared_destination_transitively() {
        let dest = |id: i64, d: Option<&str>| {
            let mut l = load(id, 1.0, vec![sound(id * 10, 8)]);
            l.destination = d.map(Into::into);
            l
        };
        let v1 = voyage(1, GIG, None, vec![dest(1, Some("Area18 TDD"))]);
        let v2 = voyage(2, JT, None, vec![dest(2, Some("Area18 TDD")), dest(3, Some("Lorville TDD"))]);
        let v3 = voyage(3, GIG, None, vec![dest(4, Some("lorville tdd"))]); // case-insensitive
        let v4 = voyage(4, GIG, None, vec![dest(5, Some("Grim HEX"))]);
        let hulled = voyage(5, GIG, Some(ship("Hull-C", "hull-c")), vec![dest(6, Some("Grim HEX"))]);
        let clusters = cluster_unassigned(&[&v4, &v3, &v2, &v1, &hulled]);
        assert_eq!(clusters, vec![vec![1, 2, 3], vec![4]]);
    }

    #[test]
    fn undecided_destination_never_pairs() {
        let dest = |id: i64, d: Option<&str>| {
            let mut l = load(id, 1.0, vec![sound(id * 10, 8)]);
            l.destination = d.map(Into::into);
            l
        };
        let v1 = voyage(1, GIG, None, vec![dest(1, None)]);
        let v2 = voyage(2, JT, None, vec![dest(2, None), dest(3, Some("  "))]);
        let v3 = voyage(3, GIG, None, vec![dest(4, Some("Area18 TDD"))]);
        assert_eq!(cluster_unassigned(&[&v1, &v2, &v3]), vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn sc_cargo_link_grammar() {
        assert_eq!(
            sc_cargo_link("hull-c", &[8, 8, 32, 32, 32, 32]),
            "https://sc-cargo.space/#/viewer/hull-c-official-r2-u4"
        );
        assert_eq!(sc_cargo_link("hull-c", &[]), "https://sc-cargo.space/#/viewer/hull-c-official");
        assert_eq!(
            sc_cargo_link("c2 hercules", &[1, 2, 4, 8, 16, 24, 32]),
            "https://sc-cargo.space/#/viewer/c2%20hercules-official-q1-w1-e1-r1-t1-y1-u1"
        );
    }

    // -- crate picker rules --------------------------------------------------

    #[test]
    fn glitched_crates_excluded_from_select_all_but_selectable() {
        let g = crate_(1, 8, Condition::Glitched, Whereabouts::Aboard);
        assert!(!g.default_selected());
        assert!(g.is_sellable());
        let l = load(1, 10.0, vec![g.clone()]);
        assert!(settle(&l, &SaleInput { crate_ids: vec![1], price_mode: PriceMode::Scu, price: 15.0, shares: vec![] }).is_ok());
    }

    #[test]
    fn lost_crates_never_sellable() {
        let lost = crate_(1, 8, Condition::Lost, Whereabouts::Aboard);
        assert!(!lost.is_sellable());
        assert!(!lost.default_selected());
        let sold = crate_(2, 8, Condition::Sold, Whereabouts::Aboard);
        assert!(!sold.is_sellable());
    }

    #[test]
    fn condition_cycles_sound_glitched_lost() {
        assert_eq!(Condition::Sound.next(), Condition::Glitched);
        assert_eq!(Condition::Glitched.next(), Condition::Lost);
        assert_eq!(Condition::Lost.next(), Condition::Sound);
        assert_eq!(Condition::Sold.next(), Condition::Sold);
    }

    #[test]
    fn parse_date_accepts_real_dates_and_normalises() {
        let today = time::Date::from_calendar_date(2026, time::Month::November, 8).unwrap();
        assert_eq!(parse_date(" 2026-11-08 ", today), Ok("2026-11-08".to_string()));
        assert_eq!(parse_date("2026-1-5", today), Ok("2026-01-05".to_string()));
        assert!(parse_date("2956-11-08", today).unwrap_err().contains("in-game"));
        assert!(parse_date("yesterday", today).is_err());
    }
}

#[cfg(test)]
mod properties {
    use super::fixtures::*;
    use super::*;
    use proptest::prelude::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
    }

    fn size() -> impl Strategy<Value = u32> {
        prop::sample::select(CRATE_SIZES.to_vec())
    }

    fn condition() -> impl Strategy<Value = Condition> {
        prop_oneof![Just(Condition::Sound), Just(Condition::Glitched), Just(Condition::Lost)]
    }

    fn whereabouts() -> impl Strategy<Value = Whereabouts> {
        prop_oneof![
            Just(Whereabouts::Aboard),
            Just(Whereabouts::Stored),
            Just(Whereabouts::AwaitingPickup)
        ]
    }

    /// 1–40 crates over the seven sizes, random condition and whereabouts.
    fn crates() -> impl Strategy<Value = Vec<Crate>> {
        prop::collection::vec((size(), condition(), whereabouts()), 1..=40).prop_map(|v| {
            v.into_iter()
                .enumerate()
                .map(|(i, (s, c, w))| crate_(i as i64 + 1, s, c, w))
                .collect()
        })
    }

    fn price() -> impl Strategy<Value = f64> {
        (0u32..=50_000).prop_map(|p| p as f64)
    }

    fn mode() -> impl Strategy<Value = PriceMode> {
        prop_oneof![Just(PriceMode::Scu), Just(PriceMode::Lot)]
    }

    fn share_input() -> impl Strategy<Value = ShareInput> {
        (
            prop::sample::select(vec![JT, THIRD]),
            prop::option::of(prop::sample::select(vec![JT, THIRD])),
            prop_oneof![Just(ShareKind::Pct), Just(ShareKind::Flat)],
            0u32..=25, // four shares at most, so percentages never exceed the whole profit
            0u32..=100_000,
        )
            .prop_map(|(to, forfeit, kind, pct, flat)| ShareInput {
                to_user: to,
                paid_to: forfeit,
                kind,
                amount: match kind {
                    ShareKind::Pct => pct as f64,
                    ShareKind::Flat => flat as f64,
                },
                reason: None,
            })
    }

    /// A load plus a random subset of its sellable crates (possibly empty).
    fn load_and_selection() -> impl Strategy<Value = (Load, Vec<i64>)> {
        (crates(), price(), prop::collection::vec(any::<bool>(), 40)).prop_map(|(cs, buy, picks)| {
            let selected: Vec<i64> = cs
                .iter()
                .zip(picks.iter())
                .filter(|(c, &p)| p && c.is_sellable())
                .map(|(c, _)| c.id)
                .collect();
            (load(1, buy, cs), selected)
        })
    }

    fn sale_input(selected: Vec<i64>) -> impl Strategy<Value = SaleInput> {
        (mode(), price(), prop::collection::vec(share_input(), 0..=4)).prop_map(move |(m, p, shares)| {
            SaleInput { crate_ids: selected.clone(), price_mode: m, price: p, shares }
        })
    }

    fn sale_case() -> impl Strategy<Value = (Load, SaleInput)> {
        load_and_selection()
            .prop_filter("needs at least one crate selected", |(_, sel)| !sel.is_empty())
            .prop_flat_map(|(l, sel)| (Just(l), sale_input(sel)))
    }

    /// Mix generator: a normalised BoxMix (each size at most once, largest first).
    fn mix() -> impl Strategy<Value = BoxMix> {
        prop::collection::vec(0u32..=6, 7).prop_map(|counts| {
            CRATE_SIZES
                .iter()
                .rev()
                .zip(counts.iter().rev())
                .filter(|(_, &n)| n > 0)
                .map(|(&s, &n)| (n, s))
                .collect()
        })
    }

    proptest! {
        /// 1. Σ share.paid + seller_keeps == profit.
        #[test]
        fn shares_plus_kept_equal_profit((l, input) in sale_case()) {
            let sale = settle(&l, &input).unwrap();
            let paid: f64 = sale.shares.iter().map(|s| s.paid).sum();
            prop_assert!(approx(paid + sale.seller_keeps, sale.profit), "{} + {} != {}", paid, sale.seller_keeps, sale.profit);
        }

        /// 2. profit <= 0 ⇒ every share.paid == 0.
        #[test]
        fn nothing_is_owed_on_a_loss((l, input) in sale_case()) {
            let sale = settle(&l, &input).unwrap();
            if sale.profit <= 0.0 {
                prop_assert!(sale.shares.iter().all(|s| s.paid == 0.0));
            }
        }

        /// 7. scu_sold <= scu and scu_sold == Σ sizes of selected crates.
        #[test]
        fn scu_sold_is_the_selected_crates((l, input) in sale_case()) {
            let sale = settle(&l, &input).unwrap();
            let expected: u32 = l.crates.iter().filter(|c| input.crate_ids.contains(&c.id)).map(|c| c.size_scu).sum();
            prop_assert_eq!(sale.scu_sold, expected);
            prop_assert!(sale.scu_sold <= l.scu());
            prop_assert_eq!(sale.sold.len() + sale.kept.len(), l.crates.len());
        }

        /// 3. Σ board_credit == realised over the org, write-offs on closed
        /// voyages included (open-voyage ones on neither side); Σ cash == the
        /// revenue of every sale, shares and write-offs aside.
        #[test]
        fn credit_and_cash_agree_across_the_org(cases in prop::collection::vec(sale_case(), 1..=4)) {
            let mut voyages = Vec::new();
            for (i, (l, input)) in cases.into_iter().enumerate() {
                let sale = settle(&l, &input).unwrap();
                let id = i as i64 + 1;
                let seller = if i % 2 == 0 { GIG } else { JT };
                let mut sold = sold_load(id, seller, l.buy_price_scu, sale.sell_price_scu, &sale.sold.iter().map(|c| c.size_scu).collect::<Vec<_>>(),
                    input.shares.iter().map(|s| (s.to_user, s.paid_to, s.kind, s.amount)).collect());
                // the load's lost crates ride along as write-offs
                sold.crates.extend(sale.kept.iter().filter(|c| c.is_lost()).cloned());
                voyages.push(voyage(id, seller, None, vec![sold]));
            }
            // an open voyage with a write-off stays out of both sides
            voyages.push(voyage(99, GIG, None, vec![load(99, 10.0, vec![sound(991, 8), crate_(992, 8, Condition::Lost, Whereabouts::Aboard)])]));
            let credit: f64 = [GIG, JT, THIRD].iter().map(|&u| board_credit(u, &voyages)).sum();
            let money: f64 = [GIG, JT, THIRD].iter().map(|&u| cash(u, &voyages)).sum();
            let revenue: f64 = voyages.iter().flat_map(|v| v.loads.iter()).filter_map(|l| l.revenue()).sum();
            prop_assert!(approx(money, revenue), "{} vs {}", money, revenue);
            let realised = Totals::from_voyages(&voyages).realised;
            prop_assert!(approx(credit, realised), "{} vs {}", credit, realised);
        }

        /// 4. committed − written_off >= 0 (the net figure is never negative).
        #[test]
        fn committed_net_of_write_offs_is_never_negative(cs in crates(), buy in price()) {
            let v = voyage(1, GIG, None, vec![load(1, buy, cs)]);
            let t = Totals::from_voyages(&[v]);
            prop_assert!(t.committed >= 0.0);
            prop_assert!(approx(t.committed_gross - t.written_off, t.committed));
        }

        /// 5. parse_boxes(format_boxes(mix)) == mix.
        #[test]
        fn boxes_round_trip(m in mix()) {
            prop_assume!(!m.is_empty());
            prop_assert_eq!(parse_boxes(&format_boxes(&m)).unwrap(), m);
        }

        /// 6. stowage_order is a permutation of the loads, non-increasing in stop_no.
        #[test]
        fn stowage_order_is_a_permutation_by_stop(stops in prop::collection::vec((1u32..=6, size()), 1..=8)) {
            let loads: Vec<Load> = stops.iter().enumerate().map(|(i, &(stop, sz))| {
                let mut l = load(i as i64 + 1, 1.0, vec![sound(i as i64 * 10 + 1, sz)]);
                l.stop_no = stop;
                l
            }).collect();
            let refs: Vec<&Load> = loads.iter().collect();
            let order = stowage_order(&refs);
            let mut ids: Vec<i64> = order.iter().map(|e| e.load_id).collect();
            ids.sort_unstable();
            prop_assert_eq!(ids, (1..=loads.len() as i64).collect::<Vec<_>>());
            prop_assert!(order.windows(2).all(|w| w[0].stop_no >= w[1].stop_no));
            prop_assert_eq!(order[0].depth, Depth::Deepest);
        }

        /// 8. Clustering partitions the unassigned voyages; shared destinations always cluster.
        #[test]
        fn clustering_is_a_partition(dests in prop::collection::vec(prop::collection::vec(0u8..=4, 1..=3), 1..=6)) {
            let names = ["", "Area18 TDD", "Lorville TDD", "Grim HEX", "Everus Harbor"];
            let voyages: Vec<Voyage> = dests.iter().enumerate().map(|(i, ds)| {
                let loads = ds.iter().enumerate().map(|(j, &d)| {
                    let mut l = load((i * 10 + j) as i64 + 1, 1.0, vec![sound(1, 8)]);
                    l.destination = (d > 0).then(|| names[d as usize].to_string());
                    l
                }).collect();
                voyage(i as i64 + 1, GIG, None, loads)
            }).collect();
            let refs: Vec<&Voyage> = voyages.iter().collect();
            let clusters = cluster_unassigned(&refs);
            let mut all: Vec<i64> = clusters.iter().flatten().copied().collect();
            all.sort_unstable();
            prop_assert_eq!(all, (1..=voyages.len() as i64).collect::<Vec<_>>());
            let cluster_of = |id: i64| clusters.iter().position(|c| c.contains(&id)).unwrap();
            for a in &voyages {
                for b in &voyages {
                    let shared = a.destinations().iter().any(|d| b.destinations().iter().any(|e| e.eq_ignore_ascii_case(d)));
                    if shared {
                        prop_assert_eq!(cluster_of(a.id), cluster_of(b.id));
                    }
                }
            }
        }
    }
}
