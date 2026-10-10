//! Groups of zones, like groups in a paint program.
//!
//! A project holds a tree of groups. A group holds forest zones and other groups. A zone that is in no group
//! follows the project settings. Whatever sits in a group is drawn with the group settings.
//!
//! Settings cascade. Each group stores only the settings it changes, as a sparse copy of `GenParams`, and takes
//! everything else from the group above it, up to the project. So changing a group changes everything inside it
//! that has not set that setting itself, and a group inside can still keep its own value. The species list is
//! the exception: it is one unit with the three species indexes in the settings (main, mix and shrub), so a
//! group that sets a list also sets those indexes.
//!
//! Drawing order: a group draws its own zones first, then the groups inside it on top, each group's zones in
//! list order. Clearings (zones that cut) cut the group they are in and everything inside it; a clearing in no
//! group cuts everything.
//!
//! `Project::effective` works out what a group really uses, and is the one place that does it.

use crate::params::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    /// Never changes and is never reused, so zones can point at a group by it.
    pub id: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub visible: bool,
    /// Shown folded in the list.
    pub collapsed: bool,
    /// The settings this group changes, a sparse nested copy of `GenParams`. Null for none.
    pub overrides: Value,
    /// This group's own species list. The three species indexes in the settings come with it.
    pub species: Option<Vec<Species>>,
    /// Settings as a full copy, from before groups kept only their changes. Read on load, never written.
    #[serde(skip_serializing)]
    pub settings: Option<ZoneSettings>,
}

impl Default for Group {
    fn default() -> Self {
        Group { id: 0, parent: None, name: String::new(), visible: true, collapsed: false, overrides: Value::Null, species: None, settings: None }
    }
}

/// The fields of the settings that go with the species list.
pub const SPECIES_KEYS: [&str; 3] = ["main_species", "mix_species", "shrub_species"];

/// Lay `over` on `base`: objects merge field by field, everything else is replaced whole.
pub fn merge_value(base: &mut Value, over: &Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(bv) if bv.is_object() && v.is_object() => merge_value(bv, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// What differs between two settings, as a sparse nested object. Null when they are the same.
pub fn diff_value(before: &Value, after: &Value) -> Value {
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => {
            let mut out = Map::new();
            for (k, av) in a {
                match b.get(k) {
                    Some(bv) => {
                        let d = diff_value(bv, av);
                        if !d.is_null() {
                            out.insert(k.clone(), d);
                        }
                    }
                    None => {
                        out.insert(k.clone(), av.clone());
                    }
                }
            }
            if out.is_empty() { Value::Null } else { Value::Object(out) }
        }
        (b, a) if b == a => Value::Null,
        (_, a) => a.clone(),
    }
}

impl Project {
    /// Everything that decides how the forest is drawn, as text, to tell when it needs drawing again. Groups
    /// are part of it, so editing a group redraws what is in it. The picture under the forest, painted trees
    /// and grading are not: they have their own ways of showing a change.
    pub fn render_key(&self) -> String {
        serde_json::to_string(&(self.map_w, self.map_h, &self.shapes, &self.groups, &self.gen, &self.light, &self.species)).unwrap_or_default()
    }

    pub fn group(&self, id: u32) -> Option<&Group> {
        self.groups.iter().find(|n| n.id == id)
    }

    pub fn group_mut(&mut self, id: u32) -> Option<&mut Group> {
        self.groups.iter_mut().find(|n| n.id == id)
    }

    /// A free id: one more than the largest, so ids are not reused while any group holds a larger one.
    fn next_id(&self) -> u32 {
        self.groups.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }

    /// The groups from the top of the tree down to this one, or empty for an unknown id. A group that loops
    /// back on itself ends the chain.
    pub fn chain(&self, id: u32) -> Vec<&Group> {
        let mut out = Vec::new();
        let mut cur = self.group(id);
        while let Some(n) = cur {
            if out.iter().any(|m: &&Group| m.id == n.id) || out.len() > 64 {
                break;
            }
            out.push(n);
            cur = n.parent.and_then(|p| self.group(p));
        }
        out.reverse();
        out
    }

    /// Settings a group would use with no changes of its own: what the group above it uses, or the project.
    pub fn inherited(&self, id: u32) -> (GenParams, Vec<Species>) {
        let chain = self.chain(id);
        let upto = chain.len().saturating_sub(1);
        self.through(&chain[..upto])
    }

    /// The settings a group uses: the project with the changes of each group above it, then its own.
    pub fn effective(&self, id: u32) -> (GenParams, Vec<Species>) {
        self.through(&self.chain(id))
    }

    fn through(&self, chain: &[&Group]) -> (GenParams, Vec<Species>) {
        let mut gen = serde_json::to_value(&self.gen).unwrap_or(Value::Null);
        let mut species = &self.species;
        for n in chain {
            if !n.overrides.is_null() {
                merge_value(&mut gen, &n.overrides);
            }
            if let Some(s) = &n.species {
                species = s;
            }
        }
        // a value that no longer fits the settings falls back to the project
        let gen = serde_json::from_value(gen).unwrap_or_else(|_| self.gen.clone());
        let mut species = species.clone();
        if species.is_empty() {
            species = self.species.clone();
        }
        (gen, species)
    }

    /// Is this group and every group above it showing?
    pub fn shown(&self, id: u32) -> bool {
        let chain = self.chain(id);
        !chain.is_empty() && chain.iter().all(|n| n.visible)
    }

    /// The group a zone is really in: None when it is in no group or the group is not there.
    pub fn zone_group(&self, s: &Shape) -> Option<u32> {
        s.group.and_then(|g| self.group(g)).map(|g| g.id)
    }

    /// The groups inside a group, however deep, not counting it.
    pub fn descendants(&self, id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(p) = stack.pop() {
            for n in self.groups.iter().filter(|n| n.parent == Some(p)) {
                if n.id != id && !out.contains(&n.id) {
                    out.push(n.id);
                    stack.push(n.id);
                }
            }
        }
        out
    }

    /// Every group in drawing order, bottom first: a group before the groups inside it, siblings in list order.
    pub fn draw_order(&self) -> Vec<u32> {
        fn walk(p: &Project, parent: Option<u32>, out: &mut Vec<u32>, depth: usize) {
            if depth > 64 {
                return;
            }
            for n in p.groups.iter().filter(|n| n.parent == parent) {
                out.push(n.id);
                walk(p, Some(n.id), out, depth + 1);
            }
        }
        let mut out = Vec::new();
        walk(self, None, &mut out, 0);
        out
    }

    /// The zones directly in a group.
    pub fn zones_in(&self, id: u32) -> usize {
        self.shapes.iter().filter(|s| s.group == Some(id)).count()
    }

    /// The zones in a group and in the groups inside it.
    pub fn zones_under(&self, id: u32) -> usize {
        let mut ids = self.descendants(id);
        ids.push(id);
        self.shapes.iter().filter(|s| s.group.is_some_and(|g| ids.contains(&g))).count()
    }

    /// A name with the groups above it, such as "Forest / Big oaks".
    pub fn path_name(&self, id: u32) -> String {
        self.chain(id).iter().map(|n| if n.name.is_empty() { "Group" } else { n.name.as_str() }).collect::<Vec<_>>().join(" / ")
    }

    /// A new zone, placed in a group when one is given and the group exists. Returns where it went.
    pub fn add_zone(&mut self, op: ShapeOp, points: Vec<[f32; 2]>, group: Option<u32>) -> usize {
        let mut z = Shape::new(op, points);
        z.group = group.filter(|g| self.group(*g).is_some());
        self.shapes.push(z);
        self.shapes.len() - 1
    }

    /// A new group on top, inside another group or at the top level.
    pub fn add_group(&mut self, parent: Option<u32>) -> u32 {
        let id = self.next_id();
        let parent = parent.filter(|p| self.group(*p).is_some());
        let name = format!("Group {}", self.groups.len() + 1);
        self.groups.push(Group { id, parent, name, ..Default::default() });
        id
    }

    /// Put a group inside another, or at the top level with None, above its new siblings. Refuses to move a group
    /// into itself or what it holds.
    pub fn move_into(&mut self, id: u32, parent: Option<u32>) -> bool {
        if let Some(p) = parent {
            if p == id || self.descendants(id).contains(&p) || self.group(p).is_none() {
                return false;
            }
        }
        let Some(i) = self.groups.iter().position(|n| n.id == id) else { return false };
        let mut n = self.groups.remove(i);
        n.parent = parent;
        self.groups.push(n);
        true
    }

    /// Swap a group with the sibling next to it in the drawing order: +1 draws it later (above), -1 earlier.
    pub fn nudge(&mut self, id: u32, dir: i32) -> bool {
        let Some(me) = self.groups.iter().position(|n| n.id == id) else { return false };
        let parent = self.groups[me].parent;
        let sibs: Vec<usize> = self.groups.iter().enumerate().filter(|(_, n)| n.parent == parent).map(|(i, _)| i).collect();
        let Some(at) = sibs.iter().position(|i| *i == me) else { return false };
        let to = at as i32 + dir;
        if to < 0 || to as usize >= sibs.len() {
            return false;
        }
        self.groups.swap(me, sibs[to as usize]);
        true
    }

    /// Wrap a group in a new group that takes its place.
    pub fn wrap_group(&mut self, id: u32) -> Option<u32> {
        let pos = self.groups.iter().position(|n| n.id == id)?;
        let parent = self.groups[pos].parent;
        let gid = self.next_id();
        let name = format!("Group {}", self.groups.len() + 1);
        self.groups.insert(pos, Group { id: gid, parent, name, ..Default::default() });
        self.groups[pos + 1].parent = Some(gid);
        Some(gid)
    }

    /// Put a zone in a new group that sits where the zone sat: inside the zone's group, or at the top.
    pub fn group_zone(&mut self, shape: usize) -> Option<u32> {
        let parent = self.shapes.get(shape)?.group.filter(|g| self.group(*g).is_some());
        let gid = self.add_group(parent);
        self.shapes[shape].group = Some(gid);
        Some(gid)
    }

    /// Take a group apart. What it holds, zones and groups, moves up to where it was, in the same order, and
    /// follows the settings of the group above, as the group's own are gone.
    pub fn ungroup(&mut self, id: u32) -> bool {
        let Some(g) = self.group(id).cloned() else { return false };
        for s in self.shapes.iter_mut() {
            if s.group == Some(id) {
                s.group = g.parent;
            }
        }
        let kids: Vec<u32> = self.groups.iter().filter(|n| n.parent == Some(id)).map(|n| n.id).collect();
        for k in &kids {
            if let Some(n) = self.group_mut(*k) {
                n.parent = g.parent;
            }
        }
        // the groups inside take this group's place in the list
        let mut out = Vec::with_capacity(self.groups.len());
        let mut moved: Vec<Group> = Vec::new();
        for n in std::mem::take(&mut self.groups) {
            if kids.contains(&n.id) {
                moved.push(n);
            } else {
                out.push(n);
            }
        }
        let at = out.iter().position(|n| n.id == id).unwrap_or(out.len());
        if at < out.len() {
            out.remove(at);
        }
        for (i, n) in moved.into_iter().enumerate() {
            out.insert(at + i, n);
        }
        self.groups = out;
        true
    }

    /// Delete a group with every zone and group inside it.
    pub fn delete_group(&mut self, id: u32) {
        let mut gone = self.descendants(id);
        gone.push(id);
        self.shapes.retain(|s| !s.group.is_some_and(|g| gone.contains(&g)));
        self.groups.retain(|n| !gone.contains(&n.id));
    }

    /// Keep on a group what the settings panel changed. `before` is what the group used when the panel started
    /// and `after` what it ended with; only the settings that differ become changes of the group, so every
    /// other setting keeps following what the group inherits. `resets` are settings (by name) put back to
    /// inherited. If the species list changed, the group keeps the new list together with the three species
    /// indexes.
    pub fn record_edit(&mut self, id: u32, before: (&GenParams, &[Species]), after: (&GenParams, &[Species]), resets: &[String]) {
        let before_gen = serde_json::to_value(before.0).unwrap_or(Value::Null);
        let after_gen = serde_json::to_value(after.0).unwrap_or(Value::Null);
        let mut changes = diff_value(&before_gen, &after_gen);
        if let Some(o) = changes.as_object_mut() {
            for k in resets {
                o.remove(k);
            }
        }
        let species_changed = serde_json::to_value(before.1).unwrap_or(Value::Null) != serde_json::to_value(after.1).unwrap_or(Value::Null);
        let Some(node) = self.group_mut(id) else { return };
        if let Some(o) = node.overrides.as_object_mut() {
            for k in resets {
                o.remove(k);
            }
        }
        if !changes.is_null() {
            if node.overrides.is_null() {
                node.overrides = Value::Object(Map::new());
            }
            merge_value(&mut node.overrides, &changes);
        }
        if species_changed {
            node.species = Some(after.1.to_vec());
            if node.overrides.is_null() {
                node.overrides = Value::Object(Map::new());
            }
            if let Some(o) = node.overrides.as_object_mut() {
                for k in SPECIES_KEYS {
                    if let Some(v) = after_gen.get(k) {
                        o.insert(k.to_string(), v.clone());
                    }
                }
            }
        }
    }

    /// Bring a project from an older file up to date: zones that carried their own settings, groups that held
    /// a full copy of the settings, and groups numbered by position. Layers from the version that had both
    /// layers and groups are groups now, and keep their zones.
    pub fn migrate_groups(&mut self) {
        // groups saved before they had ids are numbered from 1 by position, and zones pointed by position
        if !self.groups.is_empty() && self.groups.iter().all(|n| n.id == 0) {
            for (i, n) in self.groups.iter_mut().enumerate() {
                n.id = i as u32 + 1;
            }
            for s in self.shapes.iter_mut() {
                s.group = s.group.map(|l| l + 1);
            }
        }
        // zones that carried their own settings become a group each
        for i in 0..self.shapes.len() {
            if let Some(z) = self.shapes[i].settings.take() {
                let id = self.next_id();
                let name = if self.shapes[i].name.trim().is_empty() { format!("Group {}", self.groups.len() + 1) } else { self.shapes[i].name.clone() };
                self.groups.push(Group { id, name, settings: Some(z), ..Default::default() });
                self.shapes[i].group = Some(id);
            }
        }
        // a group with a full copy of the settings keeps only what differs from the project, with its species
        let global = serde_json::to_value(&self.gen).unwrap_or(Value::Null);
        let global_species = serde_json::to_value(&self.species).unwrap_or(Value::Null);
        for n in self.groups.iter_mut() {
            let Some(z) = n.settings.take() else { continue };
            let zg = serde_json::to_value(&z.gen).unwrap_or(Value::Null);
            let mut over = diff_value(&global, &zg);
            if !z.species.is_empty() && serde_json::to_value(&z.species).unwrap_or(Value::Null) != global_species {
                n.species = Some(z.species.clone());
                // the list and its indexes go together
                if over.is_null() {
                    over = Value::Object(Map::new());
                }
                for k in SPECIES_KEYS {
                    if let (Some(o), Some(v)) = (over.as_object_mut(), zg.get(k)) {
                        o.insert(k.to_string(), v.clone());
                    }
                }
            }
            n.overrides = over;
        }
        // a zone pointing at a group that is not there goes back to the project settings
        let ids: Vec<u32> = self.groups.iter().map(|n| n.id).collect();
        for s in self.shapes.iter_mut() {
            if s.group.is_some_and(|l| !ids.contains(&l)) {
                s.group = None;
            }
        }
    }
}
