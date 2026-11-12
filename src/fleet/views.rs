//! The fleet page's formatted shapes: the at-a-glance summary, the hull
//! list (filtered, sorted, carried hulls nested under their carrier), the
//! add / edit form, and the hull pick-list New Run and Edit voyage use.

use std::collections::{BTreeMap, HashMap};

use super::model::{resolve, Action, HullKind, HullStatus, Node, Resolved};
use super::repo::Hull;
use crate::stowage::model::{User, Voyage};
use crate::templates::fmt_auec;

// ---------------------------------------------------------------------------
// the hull picker on voyages
// ---------------------------------------------------------------------------

/// One option of the voyage forms' Hull select. `static/ripae.js` shows
/// only the run owner's hulls and fills the Ship field from `model`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HullChoice {
    pub id: i64,
    pub owner_id: i64,
    pub label: String,
    /// The model's name when it's on the ships list, else `""`.
    pub model: String,
}

/// Hulls that can fly (stored or out), plus `keep` (the voyage's own hull,
/// whatever became of it). Owner, then label.
pub fn hull_choices(hulls: &[Hull], keep: Option<i64>) -> Vec<HullChoice> {
    let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
    let mut out: Vec<(String, HullChoice)> = hulls
        .iter()
        .filter(|h| Some(h.id) == keep || resolve(&nodes, h.id).is_some_and(|r| r.status.in_play()))
        .map(|h| {
            let r = resolve(&nodes, h.id);
            let state = r.map(|r| status_line(&r)).unwrap_or_default();
            (
                h.owner_name.to_lowercase(),
                HullChoice {
                    id: h.id,
                    owner_id: h.owner_id,
                    label: format!("{} · {} · {}", h.owner_name.to_uppercase(), h.label(), state),
                    model: if h.ship_id.is_some() { h.model.clone() } else { String::new() },
                },
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.label.cmp(&b.1.label)));
    out.into_iter().map(|(_, c)| c).collect()
}

fn status_line(r: &Resolved) -> String {
    match (&r.location, r.status) {
        (_, HullStatus::Planned) => "on order".into(),
        (_, HullStatus::Destroyed) => "claim pending".into(),
        (Some(l), s) => format!("{} at {l}", s.label().to_lowercase()),
        (None, s) => s.label().to_lowercase(),
    }
}

/// `[STORE <hull> AT <stop>]` once its voyage has closed.
#[derive(Clone, Debug, PartialEq)]
pub struct StorePrompt {
    pub hull_id: i64,
    pub label: String,
    pub location: String,
}

/// Where a closed voyage left its hull: where the last sale happened, else
/// the stop the voyage reached.
pub fn voyage_end(v: &Voyage) -> String {
    v.loads
        .iter()
        .filter(|l| l.sold_at.is_some())
        .max_by(|a, b| a.sold_at.cmp(&b.sold_at).then(a.id.cmp(&b.id)))
        .and_then(|l| l.sell_terminal.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| v.stop_name(v.at_stop))
}

/// The prompt for a hull still out where its closed voyage left it.
pub fn store_prompt(hulls: &[Hull], voyages: &[Voyage], hull_id: i64) -> Option<StorePrompt> {
    let h = hulls.iter().find(|h| h.id == hull_id)?;
    let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
    let r = resolve(&nodes, hull_id)?;
    if r.status != HullStatus::Out || !r.carriers.is_empty() {
        return None;
    }
    let mine: Vec<&Voyage> = voyages.iter().filter(|v| v.hull_id == Some(hull_id)).collect();
    if mine.iter().any(|v| v.open_loads().next().is_some()) {
        return None;
    }
    let last = mine.iter().max_by_key(|v| v.id)?;
    if !last.is_fully_sold() {
        return None;
    }
    let end = voyage_end(last);
    let here = r.location.as_deref().unwrap_or("");
    // pulled out somewhere else since: the voyage no longer says where it is
    if !here.is_empty() && here != end && here != last.stop_name(last.at_stop) {
        return None;
    }
    Some(StorePrompt { hull_id, label: h.label(), location: end })
}

// ---------------------------------------------------------------------------
// the page
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FleetQuery {
    /// Owner's user id, `""` = everyone.
    pub owner: String,
    /// `out` / `stored` / `destroyed` / `planned` / `loaner`, `""` = all.
    pub status: String,
    /// `owner` (default) / `model` / `status` / `location`.
    pub sort: String,
}

impl FleetQuery {
    pub fn from_query(q: &str) -> Self {
        let mut f = FleetQuery::default();
        for (k, v) in form_urlencoded::parse(q.as_bytes()) {
            match k.as_ref() {
                "owner" => f.owner = v.trim().to_string(),
                "status" => f.status = v.trim().to_string(),
                "sort" => f.sort = v.trim().to_string(),
                _ => {}
            }
        }
        if !matches!(f.sort.as_str(), "owner" | "model" | "status" | "location") {
            f.sort = "owner".into();
        }
        f
    }

    /// `?owner=..&status=..` for the sort links (sort itself left off).
    pub fn base(&self) -> String {
        let mut s = form_urlencoded::Serializer::new(String::new());
        if !self.owner.is_empty() {
            s.append_pair("owner", &self.owner);
        }
        if !self.status.is_empty() {
            s.append_pair("status", &self.status);
        }
        let s = s.finish();
        if s.is_empty() { "?".into() } else { format!("?{s}&") }
    }
}

pub struct Tile {
    pub label: &'static str,
    pub key: &'static str,
    pub count: usize,
}

pub struct Place {
    pub name: String,
    pub count: usize,
    pub detail: String,
}

pub struct Summary {
    pub total: usize,
    pub tiles: Vec<Tile>,
    pub places: Vec<Place>,
    /// Cargo capacity of the hulls in play whose model is on the list.
    pub scu_fmt: String,
    /// What the pending claims cost, where the owner filled it in.
    pub claims_fmt: String,
    pub loaners: usize,
}

pub struct Ref {
    pub id: i64,
    pub label: String,
}

pub struct ActionBtn {
    pub action: &'static str,
    pub label: &'static str,
    pub class: &'static str,
}

pub struct HullRow {
    pub id: i64,
    pub depth: usize,
    pub owner_id: i64,
    pub owner_upper: String,
    pub label: String,
    pub nickname: String,
    pub model: String,
    pub maker: String,
    pub kind_label: &'static str,
    pub vehicle: bool,
    pub concept: bool,
    pub status_key: &'static str,
    pub status_label: &'static str,
    pub where_text: String,
    /// The location box's starting value: where it is now.
    pub location: String,
    pub carried: bool,
    pub capacity: String,
    pub insurance: String,
    /// On a destroyed hull: what the claim costs.
    pub claim_due: Option<String>,
    pub loaner: bool,
    pub loaner_for: Option<Ref>,
    pub loaners: Vec<Ref>,
    /// Delivered, but its loaners are still listed: offer to remove them.
    pub loaners_due_back: bool,
    pub flying: Option<String>,
    pub store_after: Option<StorePrompt>,
    pub actions: Vec<ActionBtn>,
    pub notes: String,
}

pub struct FleetView {
    pub q: FleetQuery,
    pub summary: Summary,
    pub rows: Vec<HullRow>,
    /// Hulls before the status filter (for "none match" vs "none yet").
    pub any: bool,
}

fn actions_for(r: &Resolved) -> Vec<ActionBtn> {
    let carried = !r.carriers.is_empty();
    let b = |action: Action, label: &'static str, class: &'static str| ActionBtn { action: action.as_str(), label, class };
    match r.status {
        HullStatus::Planned => vec![b(Action::Delivered, "Delivered", "")],
        HullStatus::Stored if carried => vec![b(Action::PullOut, "Pull out", "warn"), b(Action::Store, "Store", "")],
        HullStatus::Stored => vec![b(Action::PullOut, "Pull out", "warn"), b(Action::Destroyed, "Destroyed", "danger")],
        HullStatus::Out if carried => vec![
            b(Action::PullOut, "Pull out", "warn"),
            b(Action::Store, "Store", ""),
            b(Action::Destroyed, "Destroyed", "danger"),
        ],
        HullStatus::Out => vec![b(Action::Store, "Store", ""), b(Action::Destroyed, "Destroyed", "danger")],
        HullStatus::Destroyed => vec![b(Action::Claim, "Claim", "warn")],
    }
}

fn status_rank(s: HullStatus) -> usize {
    HullStatus::ALL.iter().position(|x| *x == s).unwrap_or(9)
}

pub fn fleet_view(hulls: &[Hull], voyages: &[Voyage], q: FleetQuery) -> FleetView {
    let nodes: Vec<Node> = hulls.iter().map(Hull::node).collect();
    let res: HashMap<i64, Resolved> = hulls.iter().filter_map(|h| resolve(&nodes, h.id).map(|r| (h.id, r))).collect();
    let by_id: HashMap<i64, &Hull> = hulls.iter().map(|h| (h.id, h)).collect();
    let owner: Option<i64> = q.owner.parse().ok();
    let owned: Vec<&Hull> = hulls.iter().filter(|h| owner.map_or(true, |o| h.owner_id == o)).collect();

    // -- at a glance (the owner filter applies, the status filter doesn't) --
    let count = |s: HullStatus| owned.iter().filter(|h| res[&h.id].status == s).count();
    let tiles = HullStatus::ALL
        .iter()
        .map(|s| Tile { label: s.label(), key: s.as_str(), count: count(*s) })
        .collect();
    let mut at: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for h in &owned {
        let r = &res[&h.id];
        if !r.status.in_play() {
            continue;
        }
        let name = r.location.clone().filter(|l| !l.trim().is_empty()).unwrap_or_else(|| "somewhere".into());
        let e = at.entry(name).or_default();
        e.0 += 1;
        if r.status == HullStatus::Out {
            e.1 += 1;
        }
    }
    let mut places: Vec<Place> = at
        .into_iter()
        .map(|(name, (n, out))| Place {
            name,
            count: n,
            detail: if out == n { "out".into() } else if out == 0 { "stored".into() } else { format!("{out} out") },
        })
        .collect();
    places.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    let scu: i64 = owned.iter().filter(|h| res[&h.id].status.in_play()).filter_map(|h| h.capacity_scu).sum();
    let destroyed: Vec<&&Hull> = owned.iter().filter(|h| res[&h.id].status == HullStatus::Destroyed).collect();
    let claim_cost: f64 = destroyed.iter().filter_map(|h| h.insurance_claim).sum();
    // the status tile counts the claims; this is what they'll cost
    let claims_fmt = match (destroyed.len(), claim_cost > 0.0) {
        (0, _) => "—".into(),
        (_, true) => format!("{} aUEC", fmt_auec(claim_cost)),
        (_, false) => "cost not set".into(),
    };
    let summary = Summary {
        total: owned.len(),
        tiles,
        places,
        scu_fmt: fmt_auec(scu as f64),
        claims_fmt,
        loaners: owned.iter().filter(|h| h.loaner).count(),
    };

    // -- the list: filtered, then carried hulls nested under their carrier --
    let shown: Vec<&Hull> = owned
        .iter()
        .copied()
        .filter(|h| match q.status.as_str() {
            "" => true,
            "loaner" => h.loaner,
            s => HullStatus::parse(s).is_some_and(|s| res[&h.id].status == s),
        })
        .collect();
    let shown_ids: Vec<i64> = shown.iter().map(|h| h.id).collect();
    let key = |h: &Hull| -> (String, String, String) {
        let r = &res[&h.id];
        let loc = r.location.clone().unwrap_or_default().to_lowercase();
        let owner = h.owner_name.to_lowercase();
        let label = h.label().to_lowercase();
        match q.sort.as_str() {
            "model" => (h.model.to_lowercase(), owner, label),
            "status" => (format!("{}", status_rank(r.status)), owner, label),
            // nowhere sorts last
            "location" => (if loc.is_empty() { "\u{10ffff}".into() } else { loc }, owner, label),
            _ => (owner, label, String::new()),
        }
    };
    let mut roots: Vec<&Hull> = shown
        .iter()
        .copied()
        .filter(|h| !h.carried_in.is_some_and(|c| shown_ids.contains(&c)))
        .collect();
    roots.sort_by(|a, b| key(a).cmp(&key(b)).then(a.id.cmp(&b.id)));
    let mut order: Vec<(&Hull, usize)> = Vec::new();
    fn walk<'a>(h: &'a Hull, depth: usize, shown: &[&'a Hull], order: &mut Vec<(&'a Hull, usize)>) {
        if order.iter().any(|(x, _)| x.id == h.id) {
            return;
        }
        order.push((h, depth));
        let mut kids: Vec<&Hull> = shown.iter().copied().filter(|k| k.carried_in == Some(h.id)).collect();
        kids.sort_by_key(|k| (k.label().to_lowercase(), k.id));
        for k in kids {
            walk(k, depth + 1, shown, order);
        }
    }
    for r in &roots {
        walk(r, 0, &shown, &mut order);
    }

    let open_voyage = |id: i64| {
        voyages
            .iter()
            .filter(|v| v.hull_id == Some(id) && v.open_loads().next().is_some())
            .max_by_key(|v| v.id)
    };
    let rows = order
        .into_iter()
        .map(|(h, depth)| {
            let r = &res[&h.id];
            let loc = r.location.clone().unwrap_or_default();
            let carrier = r.carriers.first().and_then(|c| by_id.get(c));
            let where_text = match (carrier, r.status) {
                (Some(c), _) => format!("in {}{}", c.label(), if loc.is_empty() { String::new() } else { format!(" · {loc}") }),
                (None, HullStatus::Planned) => "—".into(),
                (None, HullStatus::Destroyed) => if loc.is_empty() { "—".into() } else { format!("lost near {loc}") },
                (None, _) if loc.is_empty() => "location unknown".into(),
                (None, _) => loc.clone(),
            };
            let insurance = match (h.insurance_claim, h.insurance_type.as_deref().filter(|t| !t.trim().is_empty())) {
                (Some(c), Some(t)) => format!("{} aUEC · {t}", fmt_auec(c)),
                (Some(c), None) => format!("{} aUEC", fmt_auec(c)),
                (None, Some(t)) => t.to_string(),
                (None, None) => String::new(),
            };
            let loaners: Vec<Ref> = hulls
                .iter()
                .filter(|l| l.loaner && l.loaner_for == Some(h.id))
                .map(|l| Ref { id: l.id, label: l.label() })
                .collect();
            let flying = open_voyage(h.id).map(|v| {
                let stop = v.stop_name(v.at_stop);
                match v.next_stop() {
                    Some(n) => format!("Voyage {} · at {stop} · next {}", v.id, v.stop_name(n)),
                    None => format!("Voyage {} · at {stop}", v.id),
                }
            });
            HullRow {
                id: h.id,
                depth,
                owner_id: h.owner_id,
                owner_upper: h.owner_name.to_uppercase(),
                label: h.label(),
                nickname: h.nickname.clone().unwrap_or_default(),
                model: h.model.clone(),
                maker: h.manufacturer.clone().unwrap_or_default(),
                kind_label: h.kind.label(),
                vehicle: h.kind == HullKind::Vehicle,
                concept: h.concept,
                status_key: r.status.as_str(),
                status_label: r.status.label(),
                where_text,
                location: loc,
                carried: carrier.is_some(),
                capacity: h.capacity_scu.filter(|c| *c > 0).map(|c| format!("{} SCU", fmt_auec(c as f64))).unwrap_or_default(),
                claim_due: (r.status == HullStatus::Destroyed).then(|| match h.insurance_claim {
                    Some(c) => format!("claim {} aUEC", fmt_auec(c)),
                    None => "claim cost not set".into(),
                }),
                insurance,
                loaner: h.loaner,
                loaner_for: h.loaner_for.and_then(|p| by_id.get(&p)).map(|p| Ref { id: p.id, label: p.label() }),
                loaners_due_back: r.status != HullStatus::Planned && !loaners.is_empty(),
                loaners,
                flying,
                store_after: store_prompt(hulls, voyages, h.id),
                actions: actions_for(r),
                notes: h.notes.clone().unwrap_or_default(),
            }
        })
        .collect();
    FleetView { q, summary, rows, any: !owned.is_empty() }
}

// ---------------------------------------------------------------------------
// add / edit
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct HullFormView {
    /// `None` = a new hull.
    pub id: Option<i64>,
    pub owner: String,
    pub users: Vec<User>,
    pub model: String,
    pub nickname: String,
    pub kind: String,
    pub status: String,
    pub location: String,
    pub carried_in: String,
    /// Every other hull, as `(id, "OWNER · label")`.
    pub carriers: Vec<(String, String)>,
    pub loaner: bool,
    pub loaner_for: String,
    /// Hulls a loaner can be lent for: on order, not loaners themselves.
    pub pledges: Vec<(String, String)>,
    pub insurance_claim: String,
    pub insurance_type: String,
    pub notes: String,
    pub error: Option<String>,
}

impl HullFormView {
    pub fn options_from(&mut self, hulls: &[Hull]) {
        let me = self.id;
        self.carriers = hulls
            .iter()
            .filter(|h| Some(h.id) != me)
            .map(|h| (h.id.to_string(), format!("{} · {}", h.owner_name.to_uppercase(), h.label())))
            .collect();
        self.pledges = hulls
            .iter()
            .filter(|h| Some(h.id) != me && !h.loaner && (h.status == HullStatus::Planned || h.id.to_string() == self.loaner_for))
            .map(|h| (h.id.to_string(), format!("{} · {}", h.owner_name.to_uppercase(), h.label())))
            .collect();
    }

    pub fn of(h: &Hull) -> Self {
        HullFormView {
            id: Some(h.id),
            owner: h.owner_id.to_string(),
            model: h.model.clone(),
            nickname: h.nickname.clone().unwrap_or_default(),
            kind: h.kind.as_str().into(),
            status: h.status.as_str().into(),
            location: h.location.clone().unwrap_or_default(),
            carried_in: h.carried_in.map(|c| c.to_string()).unwrap_or_default(),
            loaner: h.loaner,
            loaner_for: h.loaner_for.map(|c| c.to_string()).unwrap_or_default(),
            insurance_claim: h.insurance_claim.map(|c| fmt_auec(c)).unwrap_or_default(),
            insurance_type: h.insurance_type.clone().unwrap_or_default(),
            notes: h.notes.clone().unwrap_or_default(),
            ..Default::default()
        }
    }

    pub fn statuses(&self) -> Vec<(&'static str, &'static str)> {
        [HullStatus::Stored, HullStatus::Out, HullStatus::Planned, HullStatus::Destroyed]
            .iter()
            .map(|s| (s.as_str(), s.label()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull(id: i64, owner: i64, model: &str, status: HullStatus, loc: Option<&str>, carried_in: Option<i64>) -> Hull {
        Hull {
            id,
            owner_id: owner,
            owner_name: if owner == 1 { "Lord Gig".into() } else { "JT".into() },
            ship_id: None,
            model: model.into(),
            manufacturer: None,
            capacity_scu: None,
            concept: false,
            nickname: None,
            kind: HullKind::Ship,
            status,
            location: loc.map(String::from),
            carried_in,
            loaner: false,
            loaner_for: None,
            insurance_claim: None,
            insurance_type: None,
            notes: None,
            source: None,
            external_ref: None,
        }
    }

    #[test]
    fn carried_hulls_nest_under_their_carrier_and_read_its_location() {
        let hulls = vec![
            hull(1, 1, "Ironclad", HullStatus::Out, Some("Area18 TDD"), None),
            hull(2, 1, "C8 Pisces", HullStatus::Stored, Some("Levski"), Some(1)),
            hull(3, 1, "Aurora", HullStatus::Stored, Some("Levski"), None),
        ];
        let v = fleet_view(&hulls, &[], FleetQuery::from_query(""));
        let ids: Vec<(i64, usize)> = v.rows.iter().map(|r| (r.id, r.depth)).collect();
        assert_eq!(ids, vec![(3, 0), (1, 0), (2, 1)], "Aurora sorts before Ironclad; the Pisces under its carrier");
        let pisces = &v.rows[2];
        assert_eq!(pisces.where_text, "in Ironclad · Area18 TDD");
        assert_eq!(pisces.status_key, "out");
        assert_eq!(pisces.actions.iter().map(|a| a.action).collect::<Vec<_>>(), vec!["out", "store", "destroyed"]);
        assert_eq!(v.summary.tiles.iter().find(|t| t.key == "out").unwrap().count, 2);
        assert_eq!(v.summary.places[0].name, "Area18 TDD");
        assert_eq!((v.summary.places[0].count, v.summary.places[0].detail.as_str()), (2, "out"));
    }

    #[test]
    fn filters_and_sorts() {
        let hulls = vec![
            hull(1, 1, "Ironclad", HullStatus::Out, Some("Lorville TDD"), None),
            hull(2, 2, "Hull-C", HullStatus::Stored, Some("Area18 TDD"), None),
            hull(3, 2, "Arrastra", HullStatus::Planned, None, None),
        ];
        let v = fleet_view(&hulls, &[], FleetQuery::from_query("owner=2"));
        assert_eq!(v.rows.len(), 2);
        assert_eq!(v.summary.total, 2);
        let v = fleet_view(&hulls, &[], FleetQuery::from_query("status=planned"));
        assert_eq!(v.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3]);
        assert_eq!(v.rows[0].actions[0].action, "delivered");
        let v = fleet_view(&hulls, &[], FleetQuery::from_query("sort=location"));
        assert_eq!(v.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2, 1, 3], "nowhere last");
        let v = fleet_view(&hulls, &[], FleetQuery::from_query("sort=status"));
        assert_eq!(v.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        let v = fleet_view(&hulls, &[], FleetQuery::from_query("sort=bogus"));
        assert_eq!(v.q.sort, "owner");
        assert_eq!(FleetQuery::from_query("owner=2&sort=model").base(), "?owner=2&");
    }

    #[test]
    fn a_delivered_pledge_offers_its_loaners_back() {
        let mut pledge = hull(1, 1, "Hull-D", HullStatus::Planned, None, None);
        let mut loaner = hull(2, 1, "Hull-C", HullStatus::Stored, Some("Area18 TDD"), None);
        loaner.loaner = true;
        loaner.loaner_for = Some(1);
        let v = fleet_view(&[pledge.clone(), loaner.clone()], &[], FleetQuery::default());
        let p = v.rows.iter().find(|r| r.id == 1).unwrap();
        assert_eq!(p.loaners.len(), 1);
        assert!(!p.loaners_due_back);
        assert_eq!(v.rows.iter().find(|r| r.id == 2).unwrap().loaner_for.as_ref().unwrap().label, "Hull-D");
        pledge.status = HullStatus::Stored;
        let v = fleet_view(&[pledge, loaner], &[], FleetQuery::default());
        assert!(v.rows.iter().find(|r| r.id == 1).unwrap().loaners_due_back);
        assert_eq!(v.summary.loaners, 1);
    }

    #[test]
    fn destroyed_hulls_show_their_claim_cost() {
        let mut h = hull(1, 1, "Cutlass", HullStatus::Destroyed, Some("Grim HEX"), None);
        h.insurance_claim = Some(15_000.0);
        let v = fleet_view(&[h], &[], FleetQuery::default());
        assert_eq!(v.rows[0].claim_due.as_deref(), Some("claim 15,000 aUEC"));
        assert_eq!(v.summary.claims_fmt, "15,000 aUEC");
        assert_eq!(v.rows[0].actions[0].action, "claim");
    }

    #[test]
    fn the_voyage_picker_offers_hulls_that_can_fly() {
        let hulls = vec![
            hull(1, 1, "Ironclad", HullStatus::Out, Some("Area18 TDD"), None),
            hull(2, 1, "Arrastra", HullStatus::Planned, None, None),
            hull(3, 2, "Hull-C", HullStatus::Destroyed, None, None),
        ];
        let c = hull_choices(&hulls, None);
        assert_eq!(c.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1]);
        assert_eq!(c[0].label, "LORD GIG · Ironclad · pulled out at Area18 TDD");
        assert_eq!(hull_choices(&hulls, Some(3)).len(), 2, "a voyage keeps its own hull");
    }
}
