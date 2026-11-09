//! Ending the whole-program fixpoint's oscillations.
//!
//! A round's harvested returns and unified parameter rows are the next
//! round's input, and that dependency can have a period. A constructor
//! called with readers of its own instances (`Chunk.new(start:
//! mapping[c.start] || c.start, …)`) flips `start` between `Integer` and
//! `Integer | nil` every round, its `initialize` row with it, and the
//! signature check never passes. Each loop then runs to `FIXPOINT_CAP`
//! retyping the app for a handful of entries that will never stop moving.
//!
//! So each round's movement is watched per entry, and an entry that
//! oscillates is pinned to the join of the values it moves between:
//!
//! - moved back to the value it held before its last move (a 2-cycle):
//!   the join of the two;
//! - moved in [`STREAK`] consecutive rounds, or [`MOVES_SLACK`] more
//!   rounds than that in all (a longer cycle, or a recursive type that
//!   the size bound reshapes each round, or every other round, without it
//!   ever repeating): the join, with containers below [`WIDEN_DEPTH`]
//!   cut to `untyped`. One still moving every round after that is cut
//!   shallower, [`WIDEN_STEP`] levels each [`WIDEN_STEP`] rounds, down to
//!   its top level: a cycle through several methods can add an arm a
//!   round for as long as the nesting leaves room.
//!
//! A pinned entry is joined again after every later harvest and
//! unification, so a round cannot take back what the join holds. The
//! join is a union and the cut keeps it finite, so a pinned entry can only
//! grow toward a bound, and stops. An entry that stops moving within
//! [`STREAK`] rounds, never moving back, is left exactly as it moved.

use crate::det::HashMap;
use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

use super::{body, fixpoint_bound};

/// Consecutive rounds an entry may move before it is widened. A fact
/// travelling down a call chain moves each entry on it once or twice.
const STREAK: usize = 4;

/// Containers a widened entry keeps.
const WIDEN_DEPTH: usize = 4;

/// Rounds, and levels, between tighter cuts of an entry that keeps moving.
const WIDEN_STEP: usize = 2;

/// Moves an entry may make in all, consecutive or not, before they count
/// as a streak: one that grows a little every other round never moves
/// [`STREAK`] rounds in a row.
const MOVES_SLACK: usize = 2;

/// How deep an entry that has moved `streak` rounds in a row is cut, if at all.
fn widen_depth(streak: usize) -> Option<usize> {
    (streak >= STREAK)
        .then(|| WIDEN_DEPTH.saturating_sub((streak - STREAK) / WIDEN_STEP * WIDEN_STEP))
}

/// A harvested return: the class, whether it is class-side, the method.
type ReturnKey = (ClassId, bool, Symbol);

struct Track<T> {
    /// The value this entry held before its most recent move.
    before: T,
    /// The observation that last saw it move, and how many in a row did.
    last: usize,
    streak: usize,
    /// Every move it has made.
    moves: usize,
    /// The join it is pinned to, and the depth it is cut to if widened.
    pinned: Option<(T, Option<usize>)>,
}

/// Per-entry movement across every loop of the fixpoint.
#[derive(Default)]
pub(super) struct Settle {
    returns: Entries<ReturnKey, Ty>,
    params: Entries<super::ParamKey, Vec<Ty>>,
}

struct Entries<K, V> {
    seen: usize,
    tracks: HashMap<K, Track<V>>,
    /// Entries pinned at a 2-cycle, and entries widened.
    cycles: usize,
    widened: usize,
}

impl<K, V> Default for Entries<K, V> {
    fn default() -> Self {
        Self {
            seen: 0,
            tracks: HashMap::default(),
            cycles: 0,
            widened: 0,
        }
    }
}

trait Joinable: Clone + PartialEq {
    fn join(&self, other: &Self, depth: Option<usize>) -> Option<Self>;
}

impl Joinable for Ty {
    fn join(&self, other: &Self, depth: Option<usize>) -> Option<Self> {
        let joined = body::union_of(self.clone(), other.clone());
        let joined = match depth {
            Some(levels) => fixpoint_bound::cut_to(&joined, levels),
            None => joined,
        };
        Some(fixpoint_bound::bound(joined))
    }
}

impl Joinable for Vec<Ty> {
    /// Position by position; rows of different arity are not one entry
    /// moving but two shapes of call, so they are left to unification.
    fn join(&self, other: &Self, depth: Option<usize>) -> Option<Self> {
        if self.len() != other.len() {
            return None;
        }
        self.iter()
            .zip(other)
            .map(|(a, b)| a.join(b, depth))
            .collect()
    }
}

impl<K: Clone + Eq + std::hash::Hash, V: Joinable> Entries<K, V> {
    /// Record one round's movement from `prev` to `now`, pinning `now`'s
    /// oscillating entries to their joins. A pinned entry is joined first,
    /// so it moves only when the join does. Entries absent on either side
    /// are a method appearing or a call shape changing, not a move.
    fn observe(&mut self, prev: &HashMap<K, V>, now: &mut HashMap<K, V>) {
        self.seen += 1;
        let seen = self.seen;
        for (key, track) in self.tracks.iter_mut() {
            let Some((pinned, depth)) = &mut track.pinned else {
                continue;
            };
            let Some(current) = now.get_mut(key) else {
                continue;
            };
            if let Some(joined) = pinned.join(current, *depth) {
                *current = joined.clone();
                *pinned = joined;
            }
        }
        for (key, value) in now.iter_mut() {
            let Some(old) = prev.get(key) else { continue };
            if old == value {
                continue;
            }
            let track = self.tracks.entry(key.clone()).or_insert_with(|| Track {
                before: old.clone(),
                last: 0,
                streak: 0,
                moves: 0,
                pinned: None,
            });
            track.streak = if track.last + 1 == seen {
                track.streak + 1
            } else {
                1
            };
            track.last = seen;
            track.moves += 1;
            let back = &track.before == value;
            let depth = widen_depth(track.streak.max(track.moves.saturating_sub(MOVES_SLACK)));
            let pinned_depth = track.pinned.as_ref().map(|(_, d)| *d);
            // Not pinned again at the depth it already has: only a 2-cycle
            // that is not yet pinned, or a tighter cut.
            let tighter =
                depth.is_some_and(|d| pinned_depth.is_none_or(|p| p.is_none_or(|p| d < p)));
            if (back && pinned_depth.is_none()) || tighter {
                if let Some(joined) = old.join(value, depth) {
                    if depth.is_some() {
                        self.widened += 1;
                    } else {
                        self.cycles += 1;
                    }
                    *value = joined.clone();
                    track.pinned = Some((joined, depth));
                }
            }
            track.before = old.clone();
        }
    }
}

impl Settle {
    /// Returns harvested since `prev_instance`/`prev_class` were taken.
    pub(super) fn returns(
        &mut self,
        classes: &mut HashMap<ClassId, body::ClassInfo>,
        prev_instance: &HashMap<ClassId, HashMap<Symbol, Ty>>,
        prev_class: &HashMap<ClassId, HashMap<Symbol, Ty>>,
    ) {
        let side_of = |class_side: bool| {
            if class_side {
                prev_class
            } else {
                prev_instance
            }
        };
        let mut prev: HashMap<ReturnKey, Ty> = HashMap::default();
        let mut now: HashMap<ReturnKey, Ty> = HashMap::default();
        for (id, info) in classes.iter() {
            for (class_side, table) in
                [(false, &info.instance_methods), (true, &info.class_methods)]
            {
                let Some(before) = side_of(class_side).get(id) else {
                    continue;
                };
                for (name, ty) in table {
                    if let Some(old) = before.get(name)
                        && old != ty
                    {
                        prev.insert((id.clone(), class_side, name.clone()), old.clone());
                        now.insert((id.clone(), class_side, name.clone()), ty.clone());
                    }
                }
            }
        }
        for key in self.returns.pinned_keys() {
            let (id, class_side, name) = &key;
            let Some(info) = classes.get(id) else {
                continue;
            };
            let table = if *class_side {
                &info.class_methods
            } else {
                &info.instance_methods
            };
            if let Some(ty) = table.get(name) {
                now.entry(key.clone()).or_insert_with(|| ty.clone());
                prev.entry(key).or_insert_with(|| ty.clone());
            }
        }
        let original = now.clone();
        self.returns.observe(&prev, &mut now);
        for (key, ty) in now {
            if original.get(&key) == Some(&ty) {
                continue;
            }
            let (id, class_side, name) = key;
            if let Some(info) = classes.get_mut(&id) {
                let table = if class_side {
                    &mut info.class_methods
                } else {
                    &mut info.instance_methods
                };
                table.insert(name, ty);
            }
        }
    }

    /// Parameter rows unified since `prev` was taken.
    pub(super) fn params(
        &mut self,
        params: &mut HashMap<super::ParamKey, Vec<Ty>>,
        prev: &HashMap<super::ParamKey, Vec<Ty>>,
    ) {
        self.params.observe(prev, params);
    }

    /// Entries pinned at a 2-cycle and entries widened, returns and
    /// parameter rows together.
    pub(super) fn counts(&self) -> (usize, usize) {
        (
            self.returns.cycles + self.params.cycles,
            self.returns.widened + self.params.widened,
        )
    }
}

impl<K: Clone + Eq + std::hash::Hash, V> Entries<K, V> {
    fn pinned_keys(&self) -> Vec<K> {
        self.tracks
            .iter()
            .filter(|(_, t)| t.pinned.is_some())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depth(ty: &Ty) -> usize {
        match ty {
            Ty::Array { elem } => 1 + depth(elem),
            Ty::Union { variants } => variants.iter().map(depth).max().unwrap_or(0),
            _ => 0,
        }
    }

    fn nilable(ty: Ty) -> Ty {
        body::union_of(ty, Ty::Nil)
    }

    fn key(name: &str) -> ReturnKey {
        (ClassId(Symbol::from("Chunk")), false, Symbol::from(name))
    }

    /// Feed `values` to one entry as consecutive rounds; the value the
    /// table holds after each.
    fn rounds(values: &[Ty]) -> (Vec<Ty>, Entries<ReturnKey, Ty>) {
        let mut entries: Entries<ReturnKey, Ty> = Entries::default();
        let mut held = values[0].clone();
        let mut out = Vec::new();
        for value in &values[1..] {
            let prev: HashMap<ReturnKey, Ty> = [(key("start"), held.clone())].into_iter().collect();
            let mut now: HashMap<ReturnKey, Ty> =
                [(key("start"), value.clone())].into_iter().collect();
            entries.observe(&prev, &mut now);
            held = now[&key("start")].clone();
            out.push(held.clone());
        }
        (out, entries)
    }

    #[test]
    fn an_entry_moving_back_is_pinned_to_the_join() {
        let (held, entries) = rounds(&[
            Ty::Int,
            nilable(Ty::Int),
            Ty::Int,
            nilable(Ty::Int),
            Ty::Int,
        ]);
        assert_eq!(held[0], nilable(Ty::Int));
        assert_eq!(
            held[1..],
            [nilable(Ty::Int), nilable(Ty::Int), nilable(Ty::Int)]
        );
        assert_eq!((entries.cycles, entries.widened), (1, 0));
    }

    #[test]
    fn an_entry_that_settles_is_left_as_it_moved() {
        let (held, entries) = rounds(&[Ty::Untyped, nilable(Ty::Int), Ty::Int, Ty::Int, Ty::Int]);
        assert_eq!(held, [nilable(Ty::Int), Ty::Int, Ty::Int, Ty::Int]);
        assert_eq!((entries.cycles, entries.widened), (0, 0));
    }

    // It grows one level every other round and holds in between, so it
    // never moves twice in a row.
    #[test]
    fn an_entry_growing_every_other_round_is_widened_and_then_holds() {
        let mut entries: Entries<ReturnKey, Ty> = Entries::default();
        let mut held = Ty::Int;
        let mut raw = Ty::Int;
        let mut history = Vec::new();
        for round in 0..24 {
            if round % 2 == 0 {
                raw = body::union_of(
                    Ty::Str,
                    Ty::Array {
                        elem: std::sync::Arc::new(raw),
                    },
                );
            }
            let prev: HashMap<ReturnKey, Ty> = [(key("start"), held.clone())].into_iter().collect();
            let next = if round % 2 == 0 {
                body::union_of(held.clone(), raw.clone())
            } else {
                held.clone()
            };
            let mut now: HashMap<ReturnKey, Ty> = [(key("start"), next)].into_iter().collect();
            entries.observe(&prev, &mut now);
            held = now[&key("start")].clone();
            history.push(held.clone());
        }
        assert!(entries.widened > 0);
        let last = history.last().unwrap();
        assert_eq!(
            &history[history.len() - 3],
            last,
            "still moving: {history:#?}"
        );
        assert!(
            depth(last) <= WIDEN_DEPTH,
            "{last:?} nests deeper than the widening"
        );
    }

    #[test]
    fn an_entry_moving_every_round_is_widened_and_then_holds() {
        let mut values = vec![Ty::Int];
        for _ in 0..12 {
            let last = values.last().unwrap().clone();
            values.push(body::union_of(
                Ty::Str,
                Ty::Array {
                    elem: std::sync::Arc::new(last),
                },
            ));
        }
        let (held, entries) = rounds(&values);
        assert!(entries.widened > 0);
        let last = held.last().unwrap();
        assert_eq!(&held[held.len() - 2], last, "still moving: {held:#?}");
        assert!(
            depth(last) <= WIDEN_DEPTH,
            "{last:?} nests deeper than the widening"
        );
    }

    #[test]
    fn rows_of_different_arity_are_not_joined() {
        let mut entries: Entries<u8, Vec<Ty>> = Entries::default();
        let prev: HashMap<u8, Vec<Ty>> = [(0, vec![Ty::Int])].into_iter().collect();
        let mut now: HashMap<u8, Vec<Ty>> = [(0, vec![Ty::Int, Ty::Str])].into_iter().collect();
        entries.observe(&prev, &mut now);
        let mut back: HashMap<u8, Vec<Ty>> = [(0, vec![Ty::Int])].into_iter().collect();
        entries.observe(&now.clone(), &mut back);
        assert_eq!(back[&0], vec![Ty::Int]);
        assert_eq!(entries.cycles, 0);
    }
}
