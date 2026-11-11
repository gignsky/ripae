//! The human half of test mode: a change set in words, grouped by voyage
//! ("New voyage · Caterpillar · 3 loads", "Load Laranite 32 SCU sold at
//! Area18 TDD for 1,234 aUEC", "Share for JT changed 25% → 30%"), and
//! conflicts in words too. Table-specific; anything it doesn't know falls
//! back to "<table> row N added / changed / deleted".

use std::collections::{BTreeMap, HashMap};

use super::merge::{ChangeSet, Conflict, RowMap, Snapshot, Val};
use crate::templates::fmt_auec;

/// Who and what the ids mean (read from the sandbox).
#[derive(Debug, Clone, Default)]
pub struct Names {
    pub users: HashMap<i64, String>,
    pub ships: HashMap<i64, String>,
}

/// One voyage's worth of changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub title: String,
    /// `new` / `changed` / `deleted` -- the CSS hook.
    pub kind: &'static str,
    pub lines: Vec<String>,
}

/// Where a row of the sandbox (or, for a deletion, of the baseline) hangs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Voyage(i64),
    Other,
}

struct Ctx<'a> {
    ch: &'a ChangeSet,
    base: &'a Snapshot,
    sb: &'a Snapshot,
    names: &'a Names,
}

fn int(row: &RowMap, col: &str) -> Option<i64> {
    row.get(col).and_then(Val::as_i64)
}

fn num(row: &RowMap, col: &str) -> Option<f64> {
    row.get(col).and_then(Val::as_f64)
}

fn text<'r>(row: &'r RowMap, col: &str) -> &'r str {
    row.get(col).and_then(Val::as_str).unwrap_or("")
}

/// aUEC, keeping cents only when there are some.
fn money(f: f64) -> String {
    if (f - f.round()).abs() < 1e-9 {
        fmt_auec(f)
    } else {
        let whole = fmt_auec(f.trunc());
        format!("{whole}.{:02}", ((f.abs().fract()) * 100.0).round() as i64)
    }
}

fn pct(f: f64) -> String {
    let s = format!("{f:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}%")
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

impl Ctx<'_> {
    fn user(&self, id: Option<i64>) -> String {
        match id {
            Some(id) => self.names.users.get(&id).cloned().unwrap_or_else(|| format!("crew #{id}")),
            None => "—".into(),
        }
    }

    fn ship(&self, id: Option<i64>) -> String {
        match id {
            Some(id) => self.names.ships.get(&id).cloned().unwrap_or_else(|| format!("hull #{id}")),
            None => "no hull".into(),
        }
    }

    /// A row as the sandbox has it, else as the baseline had it.
    fn row(&self, table: &str, id: i64) -> Option<&RowMap> {
        self.sb.row(table, id).or_else(|| self.base.row(table, id))
    }

    fn voyage_of_load(&self, load: i64) -> Key {
        self.row("loads", load).and_then(|l| int(l, "voyage_id")).map_or(Key::Other, Key::Voyage)
    }

    fn key(&self, table: &str, row: &RowMap) -> Key {
        match table {
            "voyages" => int(row, "id").map_or(Key::Other, Key::Voyage),
            "loads" | "voyage_crew" | "voyage_stops" => int(row, "voyage_id").map_or(Key::Other, Key::Voyage),
            "crates" | "shares" => int(row, "load_id").map_or(Key::Other, |l| self.voyage_of_load(l)),
            _ => Key::Other,
        }
    }

    /// SCU of a load's crates in `snap` (sold ones only, or all).
    fn scu(&self, snap: &Snapshot, load: i64, sold_only: bool) -> i64 {
        snap.rows("crates")
            .filter(|(_, c)| int(c, "load_id") == Some(load))
            .filter(|(_, c)| !sold_only || text(c, "condition") == "sold")
            .filter_map(|(_, c)| int(c, "size_scu"))
            .sum()
    }

    fn voyage_title(&self, id: i64) -> String {
        match self.row("voyages", id) {
            Some(v) => format!("{} · {}", self.ship(int(v, "ship_id")), text(v, "bought_at")),
            None => format!("Voyage {id}"),
        }
    }

    /// A voyage's route in `snap`, `start → stop 1 → …`; empty without one.
    fn route(snap: &Snapshot, voyage: i64) -> String {
        let mut stops: Vec<(i64, &str)> = snap
            .rows("voyage_stops")
            .filter(|(_, s)| int(s, "voyage_id") == Some(voyage))
            .map(|(_, s)| (int(s, "seq").unwrap_or(0), text(s, "terminal")))
            .collect();
        stops.sort();
        stops.iter().map(|(_, t)| if t.is_empty() { "?" } else { t }).collect::<Vec<_>>().join(" → ")
    }

    fn load_name(&self, l: &RowMap) -> String {
        let c = text(l, "commodity");
        if c.is_empty() { "cargo".into() } else { c.to_string() }
    }

    fn sold_line(&self, id: i64, l: &RowMap) -> String {
        let scu = self.scu(self.sb, id, true);
        let total = num(l, "sell_price_lot").unwrap_or_else(|| num(l, "sell_price_scu").unwrap_or(0.0) * scu as f64);
        if text(l, "kind") == "haul" {
            format!("Haul {} {scu} SCU delivered to {} for {} aUEC", self.load_name(l), text(l, "sell_terminal"), money(total))
        } else {
            format!("Load {} {scu} SCU sold at {} for {} aUEC", self.load_name(l), text(l, "sell_terminal"), money(total))
        }
    }

    fn label(col: &str) -> String {
        match col {
            "ship_id" => "hull",
            "bought_at" => "date",
            "logged_by" => "crew member",
            "entered_by" => "logged by",
            "voyage_id" => "voyage",
            "submitted_by" => "held by",
            "bought_by" => "bought by",
            "buy_terminal" => "bought at",
            "price_mode" => "priced per",
            "buy_price_scu" => "buy price/SCU",
            "buy_price_lot" => "buy price for the lot",
            "stop_no" => "stop",
            "sold_at" => "sale date",
            "sell_terminal" => "sold at",
            "sell_price_scu" => "sell price/SCU",
            "sell_price_lot" => "sale total",
            "to_user" => "earned by",
            "paid_to" => "paid to",
            "paid_auec" => "paid",
            "size_scu" => "size",
            "stored_at" => "stored at",
            "load_id" => "load",
            other => return other.replace('_', " "),
        }
        .to_string()
    }

    fn value(&self, col: &str, v: &Val) -> String {
        if v.is_null() {
            return "—".into();
        }
        match col {
            "ship_id" => self.ship(v.as_i64()),
            "logged_by" | "entered_by" | "submitted_by" | "bought_by" | "to_user" | "paid_to" => self.user(v.as_i64()),
            "voyage_id" => match v.as_i64() {
                Some(id) if self.ch.table("voyages").inserted.contains_key(&id) => "a new voyage".into(),
                Some(id) => format!("Voyage {id}"),
                None => "—".into(),
            },
            "buy_price_scu" | "buy_price_lot" | "sell_price_scu" | "sell_price_lot" | "reward" | "paid_auec" => {
                v.as_f64().map(money).unwrap_or_default()
            }
            "size_scu" => format!("{} SCU", v.as_i64().unwrap_or(0)),
            _ => match v {
                Val::Int(i) => i.to_string(),
                Val::Real(f) => money(*f),
                Val::Text(s) if s.is_empty() => "—".into(),
                Val::Text(s) => s.clone(),
                Val::Blob(b) => format!("{} bytes", b.len()),
                Val::Null => "—".into(),
            },
        }
    }

    /// `a → b` for every column that moved, bookkeeping columns aside.
    fn field_changes(&self, before: &RowMap, after: &RowMap, skip: &[&str]) -> Vec<String> {
        after
            .iter()
            .filter(|(c, _)| !matches!(c.as_str(), "id" | "created_at" | "updated_at") && !skip.contains(&c.as_str()))
            .filter_map(|(c, a)| {
                let b = before.get(c)?;
                (b != a).then(|| format!("{} {} → {}", Self::label(c), self.value(c, b), self.value(c, a)))
            })
            .collect()
    }

    fn share_amount(row: &RowMap) -> String {
        let a = num(row, "amount").unwrap_or(0.0);
        if text(row, "kind") == "flat" { format!("{} aUEC", money(a)) } else { pct(a) }
    }
}

/// The change set as voyage groups, in voyage order (new voyages last).
pub fn describe(ch: &ChangeSet, base: &Snapshot, sb: &Snapshot, names: &Names) -> Vec<Group> {
    let cx = Ctx { ch, base, sb, names };
    let mut lines: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    let mut push = |k: Key, s: String| lines.entry(k).or_default().push(s);

    let voyages = ch.table("voyages");
    let loads = ch.table("loads");

    for (id, (b, a)) in &voyages.updated {
        let f = cx.field_changes(b, a, &[]);
        if !f.is_empty() {
            push(Key::Voyage(*id), format!("Voyage details: {}", f.join(" · ")));
        }
    }

    // routes, whole: a rewrite deletes and re-adds every stop, so compare
    // what the route reads as rather than row by row
    let stops = ch.table("voyage_stops");
    let mut routed: Vec<i64> = stops
        .inserted
        .values()
        .chain(stops.deleted.values())
        .chain(stops.updated.values().flat_map(|(b, a)| [b, a]))
        .filter_map(|s| int(s, "voyage_id"))
        .collect();
    routed.sort_unstable();
    routed.dedup();
    for v in routed {
        if voyages.deleted.contains_key(&v) {
            continue; // said by the voyage's own line
        }
        let (before, after) = (Ctx::route(base, v), Ctx::route(sb, v));
        if before == after {
            continue;
        }
        let line = if before.is_empty() || voyages.inserted.contains_key(&v) {
            format!("Route: {after}")
        } else if after.is_empty() {
            format!("Route {before} removed")
        } else {
            format!("Route: {before} becomes {after}")
        };
        push(Key::Voyage(v), line);
    }

    for (id, l) in &loads.inserted {
        let k = cx.key("loads", l);
        if !l.get("sold_at").map_or(true, Val::is_null) {
            push(k, cx.sold_line(*id, l));
        } else {
            let scu = cx.scu(sb, *id, false);
            if text(l, "kind") == "haul" {
                push(k, format!("New haul {} {scu} SCU · {} → {}", cx.load_name(l), text(l, "buy_terminal"), text(l, "destination")));
            } else {
                let cost = num(l, "buy_price_lot").unwrap_or_else(|| num(l, "buy_price_scu").unwrap_or(0.0) * scu as f64);
                push(k, format!("New load {} {scu} SCU bought at {} for {} aUEC", cx.load_name(l), text(l, "buy_terminal"), money(cost)));
            }
        }
    }
    for (id, (b, a)) in &loads.updated {
        let k = cx.key("loads", a);
        let sold_now = b.get("sold_at").map_or(true, Val::is_null) && !a.get("sold_at").map_or(true, Val::is_null);
        if sold_now {
            push(k, cx.sold_line(*id, a));
        }
        // a partial close: some crates moved off onto a new, sold row; the
        // lot price left on this one is recomputed for what's kept
        let split = ch.table("crates").updated.values().any(|(cb, ca)| {
            int(cb, "load_id") == Some(*id)
                && int(ca, "load_id").map_or(false, |l| l != *id && loads.inserted.contains_key(&l))
        });
        if split {
            push(k, format!("Load {}: {} SCU kept open (the rest sold)", cx.load_name(a), cx.scu(sb, *id, false)));
        }
        let skip: &[&str] = match (sold_now, split) {
            (true, _) => &["sold_at", "sell_terminal", "sell_price_scu", "sell_price_lot", "submitted_by"],
            (false, true) => &["buy_price_lot"],
            _ => &[],
        };
        let f = cx.field_changes(b, a, skip);
        if !f.is_empty() {
            push(k, format!("Load {}: {}", cx.load_name(a), f.join(" · ")));
        }
    }
    for (id, l) in &loads.deleted {
        if voyages.deleted.contains_key(&int(l, "voyage_id").unwrap_or(-1)) {
            continue; // said by the voyage's own line
        }
        push(cx.key("loads", l), format!("Load {} {} SCU deleted", cx.load_name(l), cx.scu(base, *id, false)));
    }

    // crates, summed per load; crates of a load that is itself new or
    // deleted are already in that load's line
    let crates = ch.table("crates");
    let mut per_load: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let folded = |load: Option<i64>| load.map_or(true, |l| loads.inserted.contains_key(&l) || loads.deleted.contains_key(&l));
    let added: Vec<&RowMap> = crates.inserted.values().filter(|c| !folded(int(c, "load_id"))).collect();
    let removed: Vec<&RowMap> = crates.deleted.values().filter(|c| !folded(int(c, "load_id"))).collect();
    for (rows, verb) in [(added, "added"), (removed, "removed")] {
        let mut by: BTreeMap<i64, (usize, i64)> = BTreeMap::new();
        for c in rows {
            let e = by.entry(int(c, "load_id").unwrap_or(0)).or_default();
            e.0 += 1;
            e.1 += int(c, "size_scu").unwrap_or(0);
        }
        for (load, (n, scu)) in by {
            per_load.entry(load).or_default().push(format!("{} {verb} ({scu} SCU)", plural(n, "crate")));
        }
    }
    let mut moves: BTreeMap<(i64, String), usize> = BTreeMap::new();
    for (b, a) in crates.updated.values() {
        let load = int(a, "load_id").unwrap_or(0);
        if folded(Some(load)) {
            continue; // e.g. sold crates split off onto the new sold row
        }
        if text(b, "condition") != text(a, "condition") && text(a, "condition") != "sold" {
            *moves.entry((load, text(a, "condition").to_string())).or_default() += 1;
        }
        if text(b, "whereabouts") != text(a, "whereabouts") || text(b, "stored_at") != text(a, "stored_at") {
            let w = match text(a, "whereabouts") {
                "stored" => format!("stored at {}", text(a, "stored_at")),
                "awaiting_pickup" => "awaiting pickup".into(),
                other => other.replace('_', " "),
            };
            *moves.entry((load, w)).or_default() += 1;
        }
    }
    for ((load, what), n) in moves {
        per_load.entry(load).or_default().push(format!("{} {what}", plural(n, "crate")));
    }
    for (load, parts) in per_load {
        let name = cx.row("loads", load).map(|l| cx.load_name(l)).unwrap_or_else(|| "cargo".into());
        push(cx.voyage_of_load(load), format!("{name} crates: {}", parts.join(" · ")));
    }

    for (table, what) in [("shares", "Share")] {
        let sh = ch.table(table);
        let on = |r: &RowMap| match table {
            "shares" => int(r, "load_id").and_then(|l| cx.row("loads", l)).map(|l| format!(" on {}", cx.load_name(l))).unwrap_or_default(),
            _ => String::new(),
        };
        for r in sh.inserted.values() {
            let mut s = format!("{what} for {}{}: {}", cx.user(int(r, "to_user")), on(r), Ctx::share_amount(r));
            if let Some(p) = num(r, "paid_auec") {
                s.push_str(&format!(" · {} aUEC", money(p)));
            }
            if int(r, "paid_to") != int(r, "to_user") {
                s.push_str(&format!(" (forfeited to {})", cx.user(int(r, "paid_to"))));
            }
            push(cx.key(table, r), s);
        }
        for (b, a) in sh.updated.values() {
            let mut parts = Vec::new();
            if Ctx::share_amount(b) != Ctx::share_amount(a) {
                parts.push(format!("changed {} → {}", Ctx::share_amount(b), Ctx::share_amount(a)));
            }
            let rest = cx.field_changes(b, a, &["amount", "kind"]);
            parts.extend(rest);
            if !parts.is_empty() {
                push(cx.key(table, a), format!("{what} for {}{} {}", cx.user(int(a, "to_user")), on(a), parts.join(" · ")));
            }
        }
        for r in sh.deleted.values() {
            let parent_gone = match table {
                "shares" => int(r, "load_id").map_or(false, |l| loads.deleted.contains_key(&l)),
                _ => int(r, "voyage_id").map_or(false, |v| voyages.deleted.contains_key(&v)),
            };
            if !parent_gone {
                push(cx.key(table, r), format!("{what} for {}{} removed ({})", cx.user(int(r, "to_user")), on(r), Ctx::share_amount(r)));
            }
        }
    }

    // a voyage's crew: saving the voyage form replaces its rows wholesale,
    // so a member deleted and inserted again reads as one change, or none
    let crew = ch.table("voyage_crew");
    let terms = |r: &RowMap| {
        let (p, f) = (num(r, "pct").unwrap_or(0.0), num(r, "flat").unwrap_or(0.0));
        let mut t = Vec::new();
        if p > 0.0 {
            t.push(format!("{} of profit", pct(p)));
        }
        if f > 0.0 {
            t.push(format!("{} aUEC a sale", money(f)));
        }
        let mut s = if t.is_empty() { "no share".to_string() } else { t.join(" + ") };
        if let Some(to) = int(r, "paid_to") {
            s.push_str(&format!(", forfeited to {}", cx.user(Some(to))));
        }
        s
    };
    let member = |r: &RowMap| (int(r, "voyage_id"), int(r, "user_id"));
    let mut left: Vec<&RowMap> = crew
        .deleted
        .values()
        .filter(|r| !int(r, "voyage_id").map_or(false, |v| voyages.deleted.contains_key(&v)))
        .collect();
    let changed = |b: &RowMap, a: &RowMap| {
        let who = cx.user(int(a, "user_id"));
        if terms(b) != terms(a) {
            Some(format!("Crew: {who}'s terms {} → {}", terms(b), terms(a)))
        } else if text(b, "reason") != text(a, "reason") {
            Some(format!("Crew: {who}'s reason \"{}\" → \"{}\"", text(b, "reason"), text(a, "reason")))
        } else {
            None
        }
    };
    for r in crew.inserted.values() {
        match left.iter().position(|g| member(g) == member(r)).map(|i| left.remove(i)) {
            Some(b) => {
                if let Some(s) = changed(b, r) {
                    push(cx.key("voyage_crew", r), s);
                }
            }
            None => push(cx.key("voyage_crew", r), format!("Crew: {} joins · {}", cx.user(int(r, "user_id")), terms(r))),
        }
    }
    for (b, a) in crew.updated.values() {
        if let Some(s) = changed(b, a) {
            push(cx.key("voyage_crew", a), s);
        }
    }
    for r in left {
        push(cx.key("voyage_crew", r), format!("Crew: {} leaves ({})", cx.user(int(r, "user_id")), terms(r)));
    }

    // anything a later table adds, plainly
    for (table, t) in &ch.tables {
        if matches!(table.as_str(), "voyages" | "voyage_stops" | "loads" | "crates" | "shares" | "voyage_crew") {
            continue;
        }
        for id in t.inserted.keys() {
            push(Key::Other, format!("{table} row {id} added"));
        }
        for id in t.updated.keys() {
            push(Key::Other, format!("{table} row {id} changed"));
        }
        for id in t.deleted.keys() {
            push(Key::Other, format!("{table} row {id} deleted"));
        }
    }

    let mut groups: Vec<(u8, i64, Group)> = Vec::new();
    let mut keys: Vec<Key> = lines.keys().copied().collect();
    for id in voyages.inserted.keys().chain(voyages.deleted.keys()) {
        if !keys.contains(&Key::Voyage(*id)) {
            keys.push(Key::Voyage(*id));
        }
    }
    for k in keys {
        let body = lines.remove(&k).unwrap_or_default();
        let (order, id, title, kind) = match k {
            Key::Voyage(id) if voyages.inserted.contains_key(&id) => {
                let n = sb.rows("loads").filter(|(_, l)| int(l, "voyage_id") == Some(id)).count();
                (1, id, format!("New voyage · {} · {}", cx.voyage_title(id), plural(n, "load")), "new")
            }
            Key::Voyage(id) if voyages.deleted.contains_key(&id) => {
                let gone: Vec<i64> = base.rows("loads").filter(|(_, l)| int(l, "voyage_id") == Some(id)).map(|(l, _)| *l).collect();
                let scu: i64 = gone.iter().map(|l| cx.scu(base, *l, false)).sum();
                let title = format!("Voyage {id} deleted · {} · {} · {scu} SCU", cx.voyage_title(id), plural(gone.len(), "load"));
                (0, id, title, "deleted")
            }
            Key::Voyage(id) => (0, id, format!("Voyage {id} · {}", cx.voyage_title(id)), "changed"),
            Key::Other => (2, 0, "Other changes".to_string(), "changed"),
        };
        groups.push((order, id, Group { title, kind, lines: body }));
    }
    groups.sort_by_key(|(o, id, _)| (*o, *id));
    groups.into_iter().map(|(_, _, g)| g).collect()
}

/// A row as people would name it.
fn row_label(cx: &Ctx, table: &str, id: i64) -> String {
    let Some(r) = cx.row(table, id) else { return format!("{table} row {id}") };
    let voyage = |v: Option<i64>| v.map(|v| format!("Voyage {v}")).unwrap_or_default();
    match table {
        "voyages" => format!("Voyage {id} ({})", cx.voyage_title(id)),
        "loads" => format!("the {} load on {}", cx.load_name(r), voyage(int(r, "voyage_id"))),
        "crates" => match int(r, "load_id").and_then(|l| cx.row("loads", l)) {
            Some(l) => format!("a {} SCU crate of {} on {}", int(r, "size_scu").unwrap_or(0), cx.load_name(l), voyage(int(l, "voyage_id"))),
            None => format!("crate {id}"),
        },
        "shares" => match int(r, "load_id").and_then(|l| cx.row("loads", l)) {
            Some(l) => format!("{}'s share on {} ({})", cx.user(int(r, "to_user")), cx.load_name(l), voyage(int(l, "voyage_id"))),
            None => format!("{}'s share", cx.user(int(r, "to_user"))),
        },
        "voyage_crew" => format!("{}'s place on the crew of {}", cx.user(int(r, "user_id")), voyage(int(r, "voyage_id"))),
        "voyage_stops" => format!("the route of {} (stop {}, {})", voyage(int(r, "voyage_id")), int(r, "seq").unwrap_or(0), text(r, "terminal")),
        _ => format!("{table} row {id}"),
    }
}

/// Each conflict as a sentence. `live` names rows only live has.
pub fn conflict_lines(conflicts: &[Conflict], base: &Snapshot, sb: &Snapshot, live: &Snapshot, names: &Names) -> Vec<String> {
    let empty = ChangeSet::default();
    let cx = Ctx { ch: &empty, base, sb, names };
    let lx = Ctx { ch: &empty, base: live, sb: live, names };
    conflicts
        .iter()
        .map(|c| match c {
            Conflict::ChangedInLive { table, id } => {
                format!("{} was changed in live since you entered test mode.", capital(&row_label(&cx, table, *id)))
            }
            Conflict::DeletedInLive { table, id } => {
                format!("{} was deleted in live since you entered test mode.", capital(&row_label(&cx, table, *id)))
            }
            Conflict::ParentGone { table, id, parent, parent_id } => format!(
                "{} belongs to {}, which was deleted in live.",
                capital(&row_label(&cx, table, *id)),
                row_label(&cx, parent, *parent_id)
            ),
            Conflict::ChildInLive { table, id, child, child_id } => format!(
                "{} was deleted in test mode, but live has since added {} to it.",
                capital(&row_label(&cx, table, *id)),
                row_label(&lx, child, *child_id)
            ),
        })
        .collect()
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
