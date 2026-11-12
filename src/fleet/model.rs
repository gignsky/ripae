//! The fleet's rules, free of SQL: what a quick action does to a hull's
//! status, where a carried hull really is, and which carries would make a
//! loop. `repo` stores what these decide; `views` formats it.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullKind {
    Ship,
    Vehicle,
}

impl HullKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HullKind::Ship => "ship",
            HullKind::Vehicle => "vehicle",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "ship" | "" => Some(HullKind::Ship),
            "vehicle" | "ground" | "ground_vehicle" => Some(HullKind::Vehicle),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HullKind::Ship => "Ship",
            HullKind::Vehicle => "Ground vehicle",
        }
    }
}

/// One status at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HullStatus {
    /// Pledged / concept / on order, not yet delivered.
    Planned,
    Stored,
    /// Pulled out (by its owner -- nobody can pull a crewmate's hull).
    Out,
    /// Destroyed, claim pending.
    Destroyed,
}

impl HullStatus {
    pub const ALL: [HullStatus; 4] = [HullStatus::Out, HullStatus::Stored, HullStatus::Destroyed, HullStatus::Planned];

    pub fn as_str(&self) -> &'static str {
        match self {
            HullStatus::Planned => "planned",
            HullStatus::Stored => "stored",
            HullStatus::Out => "out",
            HullStatus::Destroyed => "destroyed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "planned" => Some(HullStatus::Planned),
            "stored" => Some(HullStatus::Stored),
            "out" => Some(HullStatus::Out),
            "destroyed" => Some(HullStatus::Destroyed),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HullStatus::Planned => "On order",
            HullStatus::Stored => "Stored",
            HullStatus::Out => "Pulled out",
            HullStatus::Destroyed => "Claim pending",
        }
    }

    /// Can sit in, or carry, another hull: it exists in the 'verse.
    pub fn in_play(&self) -> bool {
        matches!(self, HullStatus::Stored | HullStatus::Out)
    }
}

/// The per-hull quick actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Delivered,
    PullOut,
    Store,
    Destroyed,
    Claim,
}

impl Action {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "delivered" => Some(Action::Delivered),
            "out" | "pull_out" => Some(Action::PullOut),
            "store" => Some(Action::Store),
            "destroyed" => Some(Action::Destroyed),
            "claim" => Some(Action::Claim),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Delivered => "delivered",
            Action::PullOut => "out",
            Action::Store => "store",
            Action::Destroyed => "destroyed",
            Action::Claim => "claim",
        }
    }

    /// Takes the location field (the others ignore it).
    pub fn takes_location(&self) -> bool {
        !matches!(self, Action::Destroyed)
    }
}

/// What `action` does to a hull whose (effective) status is `from`.
/// `carried`: it rides in another hull -- pulling it out or storing it then
/// unloads it, whatever the carrier is doing.
pub fn apply(from: HullStatus, action: Action, carried: bool) -> Result<HullStatus, String> {
    use HullStatus::*;
    let to = match (action, from) {
        (Action::Delivered, Planned) => Stored,
        (Action::Delivered, _) => return Err("Only a hull on order can be delivered.".into()),
        (Action::PullOut, Stored) => Out,
        (Action::PullOut, Out) if carried => Out,
        (Action::PullOut, Out) => return Err("It's already pulled out -- store it first to move it.".into()),
        (Action::PullOut, _) => return Err(not_in_play(from)),
        // storing a stored hull is a hangar transfer
        (Action::Store, Stored | Out) => Stored,
        (Action::Store, _) => return Err(not_in_play(from)),
        (Action::Destroyed, Stored | Out) => Destroyed,
        (Action::Destroyed, Destroyed) => return Err("It's already destroyed; claim it.".into()),
        (Action::Destroyed, Planned) => return Err("A hull on order can't be destroyed.".into()),
        (Action::Claim, Destroyed) => Stored,
        (Action::Claim, _) => return Err("Only a destroyed hull can be claimed.".into()),
    };
    Ok(to)
}

fn not_in_play(s: HullStatus) -> String {
    match s {
        HullStatus::Planned => "It's still on order -- mark it delivered first.".into(),
        HullStatus::Destroyed => "It's destroyed -- claim it first.".into(),
        _ => "That doesn't apply.".into(),
    }
}

/// What the rules need of a hull.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: i64,
    pub status: HullStatus,
    pub location: Option<String>,
    pub carried_in: Option<i64>,
}

/// Where a hull really is: a carried hull takes its outermost carrier's
/// status and location.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub status: HullStatus,
    pub location: Option<String>,
    /// Carriers, innermost first (empty when it isn't carried).
    pub carriers: Vec<i64>,
}

pub fn resolve(nodes: &[Node], id: i64) -> Option<Resolved> {
    let mut cur = nodes.iter().find(|n| n.id == id)?;
    let mut carriers = Vec::new();
    let mut seen = HashSet::from([id]);
    while let Some(c) = cur.carried_in {
        // a loop (never written, but never trusted either) stops the walk
        let Some(next) = nodes.iter().find(|n| n.id == c) else { break };
        if !seen.insert(c) {
            break;
        }
        carriers.push(c);
        cur = next;
    }
    Some(Resolved { status: cur.status, location: cur.location.clone(), carriers })
}

/// Would putting `hull` inside `carrier` make a loop (or put it in itself)?
pub fn would_cycle(nodes: &[Node], hull: i64, carrier: i64) -> bool {
    if hull == carrier {
        return true;
    }
    let mut cur = Some(carrier);
    let mut seen = HashSet::new();
    while let Some(c) = cur {
        if c == hull {
            return true;
        }
        if !seen.insert(c) {
            return true;
        }
        cur = nodes.iter().find(|n| n.id == c).and_then(|n| n.carried_in);
    }
    false
}

/// Everything riding in `id`, all the way down (not `id` itself).
pub fn contents(nodes: &[Node], id: i64) -> Vec<i64> {
    let mut out = Vec::new();
    let mut stack = vec![id];
    while let Some(c) = stack.pop() {
        for n in nodes.iter().filter(|n| n.carried_in == Some(c)) {
            if n.id != id && !out.contains(&n.id) {
                out.push(n.id);
                stack.push(n.id);
            }
        }
    }
    out
}

/// Checks `hull` (in its new status) may ride in `carrier`.
pub fn check_carry(nodes: &[Node], hull: i64, own_status: HullStatus, carrier: i64) -> Result<(), String> {
    if !own_status.in_play() {
        return Err(format!("A hull that is {} can't be carried.", own_status.label().to_lowercase()));
    }
    if would_cycle(nodes, hull, carrier) {
        return Err("That carrier is inside this hull -- a hull can't carry what carries it.".into());
    }
    match resolve(nodes, carrier) {
        None => Err("That carrier doesn't exist.".into()),
        Some(r) if !r.status.in_play() => {
            Err(format!("The carrier is {} -- it can't carry anything.", r.status.label().to_lowercase()))
        }
        Some(_) => Ok(()),
    }
}

/// `"Nickname" (Model)`, or the model alone.
pub fn hull_label(model: &str, nickname: Option<&str>) -> String {
    match nickname.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => format!("{n} ({model})"),
        None => model.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HullStatus::*;

    fn node(id: i64, status: HullStatus, loc: Option<&str>, carried_in: Option<i64>) -> Node {
        Node { id, status, location: loc.map(String::from), carried_in }
    }

    #[test]
    fn quick_actions_walk_the_status_cycle() {
        assert_eq!(apply(Planned, Action::Delivered, false), Ok(Stored));
        assert_eq!(apply(Stored, Action::PullOut, false), Ok(Out));
        assert_eq!(apply(Out, Action::Store, false), Ok(Stored));
        assert_eq!(apply(Stored, Action::Store, false), Ok(Stored), "a hangar transfer");
        assert_eq!(apply(Out, Action::Destroyed, false), Ok(Destroyed));
        assert_eq!(apply(Destroyed, Action::Claim, false), Ok(Stored));
    }

    #[test]
    fn quick_actions_refuse_what_does_not_apply() {
        assert!(apply(Stored, Action::Delivered, false).is_err());
        assert!(apply(Planned, Action::PullOut, false).unwrap_err().contains("delivered"));
        assert!(apply(Destroyed, Action::PullOut, false).unwrap_err().contains("claim"));
        assert!(apply(Out, Action::PullOut, false).is_err());
        assert!(apply(Planned, Action::Destroyed, false).is_err());
        assert!(apply(Destroyed, Action::Destroyed, false).is_err());
        assert!(apply(Stored, Action::Claim, false).is_err());
        assert!(apply(Planned, Action::Store, false).is_err());
    }

    #[test]
    fn pulling_out_a_carried_hull_unloads_it_even_when_the_carrier_is_out() {
        assert_eq!(apply(Out, Action::PullOut, true), Ok(Out));
    }

    #[test]
    fn a_carried_hull_is_where_its_outermost_carrier_is() {
        // Pisces (3) in a Carrack (2) in an Ironclad (1) parked at Area18
        let nodes = vec![
            node(1, Out, Some("Area18 TDD"), None),
            node(2, Stored, Some("Lorville TDD"), Some(1)),
            node(3, Stored, Some("Levski"), Some(2)),
            node(4, Stored, Some("Levski"), None),
        ];
        let r = resolve(&nodes, 3).unwrap();
        assert_eq!(r.status, Out);
        assert_eq!(r.location.as_deref(), Some("Area18 TDD"));
        assert_eq!(r.carriers, vec![2, 1]);
        let r = resolve(&nodes, 4).unwrap();
        assert_eq!((r.status, r.location.as_deref(), r.carriers.len()), (Stored, Some("Levski"), 0));
        assert!(resolve(&nodes, 9).is_none());
    }

    #[test]
    fn resolve_survives_a_loop_in_the_data() {
        let nodes = vec![node(1, Out, Some("A"), Some(2)), node(2, Stored, Some("B"), Some(1))];
        let r = resolve(&nodes, 1).unwrap();
        assert_eq!(r.carriers, vec![2]);
        assert_eq!(r.status, Stored);
    }

    #[test]
    fn cycles_are_refused() {
        let nodes = vec![node(1, Out, None, None), node(2, Out, None, Some(1)), node(3, Out, None, Some(2)), node(4, Out, None, None)];
        assert!(would_cycle(&nodes, 1, 1), "itself");
        assert!(would_cycle(&nodes, 1, 3), "1 carries 2 carries 3");
        assert!(would_cycle(&nodes, 2, 3));
        assert!(!would_cycle(&nodes, 3, 1));
        assert!(!would_cycle(&nodes, 4, 3));
        assert!(!would_cycle(&nodes, 1, 4));
        assert!(check_carry(&nodes, 1, Out, 3).unwrap_err().contains("carries it"));
        assert!(check_carry(&nodes, 4, Out, 3).is_ok());
    }

    #[test]
    fn only_hulls_in_play_carry_or_ride() {
        let nodes = vec![node(1, Destroyed, None, None), node(2, Stored, None, None), node(3, Planned, None, None)];
        assert!(check_carry(&nodes, 2, Stored, 1).unwrap_err().contains("claim pending"));
        assert!(check_carry(&nodes, 3, Planned, 2).unwrap_err().contains("on order"));
        assert!(check_carry(&nodes, 2, Stored, 9).is_err());
    }

    #[test]
    fn contents_go_all_the_way_down() {
        let nodes = vec![node(1, Out, None, None), node(2, Out, None, Some(1)), node(3, Out, None, Some(2)), node(4, Out, None, Some(1)), node(5, Out, None, None)];
        let mut c = contents(&nodes, 1);
        c.sort();
        assert_eq!(c, vec![2, 3, 4]);
        assert!(contents(&nodes, 5).is_empty());
    }

    #[test]
    fn labels_lead_with_the_nickname() {
        assert_eq!(hull_label("Ironclad", Some("Big Iron")), "Big Iron (Ironclad)");
        assert_eq!(hull_label("Ironclad", Some("  ")), "Ironclad");
        assert_eq!(hull_label("C8 Pisces", None), "C8 Pisces");
    }

    #[test]
    fn statuses_and_kinds_round_trip() {
        for s in HullStatus::ALL {
            assert_eq!(HullStatus::parse(s.as_str()), Some(s));
        }
        for k in [HullKind::Ship, HullKind::Vehicle] {
            assert_eq!(HullKind::parse(k.as_str()), Some(k));
        }
        for a in [Action::Delivered, Action::PullOut, Action::Store, Action::Destroyed, Action::Claim] {
            assert_eq!(Action::parse(a.as_str()), Some(a));
        }
    }
}
