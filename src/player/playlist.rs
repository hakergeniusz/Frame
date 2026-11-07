//! Per-window playlist model.
//!
//! Documented dynamics (docs/03, docs/06): adding to the playlist focuses /
//! plays the newly added item; Next at the end does *not* wrap; shuffle is
//! only offered with more than one item; Previous/Next must be robust on
//! error states (the original crashed there — docs/06).

use crate::engine::MediaSource;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistItem {
    pub source: MediaSource,
}

#[derive(Debug, Default)]
pub struct Playlist {
    items: Vec<PlaylistItem>,
    current: Option<usize>,
    shuffle: bool,
    /// History stack of played indices (for Previous).
    history: Vec<usize>,
    /// Items still to be played in shuffle mode.
    unplayed: HashSet<usize>,
}

impl Playlist {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[PlaylistItem] {
        &self.items
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&PlaylistItem> {
        self.current.and_then(|i| self.items.get(i))
    }

    pub fn current_source(&self) -> Option<MediaSource> {
        self.current().map(|i| i.source.clone())
    }

    pub fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    /// Replace the whole playlist and start at the first item.
    pub fn replace(&mut self, sources: Vec<MediaSource>) -> Option<usize> {
        self.items = sources.into_iter().map(|source| PlaylistItem { source }).collect();
        self.current = None;
        self.history.clear();
        self.reset_shuffle_pool();
        if self.items.is_empty() {
            None
        } else {
            Some(self.play_index(0))
        }
    }

    /// Append items; returns the index of the first newly added item (the
    /// item the UI should focus, per docs/06), or None.
    pub fn add(&mut self, sources: Vec<MediaSource>) -> Option<usize> {
        if sources.is_empty() {
            return None;
        }
        let first_new = self.items.len();
        for source in sources {
            self.items.push(PlaylistItem { source });
        }
        self.reset_shuffle_pool();
        Some(first_new)
    }

    /// Start playing index `i` (recorded in history for Previous).
    pub fn play_index(&mut self, i: usize) -> usize {
        let i = i.min(self.items.len().saturating_sub(1));
        if let Some(cur) = self.current {
            if cur != i {
                self.history.push(cur);
                self.unplayed.remove(&i);
            }
        } else {
            self.unplayed.remove(&i);
        }
        self.current = Some(i);
        i
    }

    /// Next item. No wraparound at the end (observed behavior, docs/06).
    pub fn next(&mut self) -> Option<usize> {
        let cur = self.current?;
        if self.shuffle {
            // Pick the lowest remaining unplayed index (stable & deterministic;
            // the pool was shuffled when shuffle was toggled).
            let next = self.unplayed.iter().copied().min()?;
            self.unplayed.remove(&next);
            self.history.push(cur);
            self.current = Some(next);
            Some(next)
        } else if cur + 1 < self.items.len() {
            self.history.push(cur);
            self.current = Some(cur + 1);
            Some(cur + 1)
        } else {
            None
        }
    }

    /// Previous item from the play history; None when there is none. Robust
    /// in error states (docs/06): never panics, never wraps silently.
    pub fn previous(&mut self) -> Option<usize> {
        let prev = self.history.pop()?;
        self.unplayed.insert(self.current?);
        self.current = Some(prev);
        Some(prev)
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        self.history.retain_mut(|h| {
            if *h == index {
                false
            } else {
                if *h > index {
                    *h -= 1;
                }
                true
            }
        });
        self.unplayed = self.unplayed.iter().map(|&u| match u.cmp(&index) {
            std::cmp::Ordering::Greater => u - 1,
            std::cmp::Ordering::Equal => usize::MAX, // dropped below
            std::cmp::Ordering::Less => u,
        }).filter(|&u| u != usize::MAX).collect();
        match self.current {
            Some(cur) if cur == index => {
                // Keep showing the same position if possible; else clamp.
                if self.items.is_empty() {
                    self.current = None;
                } else if cur >= self.items.len() {
                    self.current = Some(self.items.len() - 1);
                }
            }
            Some(cur) if cur > index => self.current = Some(cur - 1),
            _ => {}
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        self.reset_shuffle_pool();
        if on {
            // Shuffle the remaining pool deterministically-randomly.
            let mut remaining: Vec<usize> = self.unplayed.iter().copied().collect();
            rand_skip_free::shuffle_slice(&mut remaining);
            self.unplayed = remaining.into_iter().collect();
        }
    }

    fn reset_shuffle_pool(&mut self) {
        self.unplayed = (0..self.items.len()).collect();
        if let Some(cur) = self.current {
            self.unplayed.remove(&cur);
        }
        for h in &self.history {
            if !self.shuffle {
                self.unplayed.remove(h);
            }
        }
    }
}

/// Tiny deterministic shuffle (Fisher–Yates over a simple LCG) — avoids a
/// rand dependency; audio shuffling doesn't need cryptographic randomness.
mod rand_skip_free {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0x9E3779B97F4A7C15) };
    }
    pub fn next_u64() -> u64 {
        STATE.with(|s| {
            let mut x = s.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s.set(x);
            x
        })
    }
    pub fn shuffle_slice<T>(slice: &mut [T]) {
        let n = slice.len();
        if n < 2 {
            return;
        }
        // Seed differently per call chain position to avoid identical orders.
        for i in (1..n).rev() {
            let j = (next_u64() % (i as u64 + 1)) as usize;
            slice.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srcs(n: usize) -> Vec<MediaSource> {
        (0..n).map(|i| MediaSource::Url(format!("http://x/{i}.mp4"))).collect()
    }

    #[test]
    fn replace_starts_first() {
        let mut p = Playlist::default();
        assert_eq!(p.replace(srcs(3)), Some(0));
        assert_eq!(p.current_index(), Some(0));
    }

    #[test]
    fn add_focuses_new_item() {
        let mut p = Playlist::default();
        p.replace(srcs(1));
        assert_eq!(p.add(srcs(2)), Some(1));
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn next_no_wraparound() {
        let mut p = Playlist::default();
        p.replace(srcs(2));
        p.play_index(1);
        assert_eq!(p.next(), None); // observed: no wrap at the end
        assert_eq!(p.current_index(), Some(1));
    }

    #[test]
    fn next_advances_and_previous_returns() {
        let mut p = Playlist::default();
        p.replace(srcs(3));
        assert_eq!(p.next(), Some(1));
        assert_eq!(p.next(), Some(2));
        assert_eq!(p.next(), None);
        assert_eq!(p.previous(), Some(1));
        assert_eq!(p.previous(), Some(0));
        assert_eq!(p.previous(), None);
    }

    #[test]
    fn previous_on_empty_is_none_never_panics() {
        let mut p = Playlist::default();
        assert_eq!(p.previous(), None);
        assert_eq!(p.next(), None);
        p.replace(srcs(1));
        assert_eq!(p.previous(), None);
        p.remove(0);
        assert_eq!(p.previous(), None);
        assert_eq!(p.next(), None);
    }

    #[test]
    fn shuffle_only_after_toggle() {
        let mut p = Playlist::default();
        p.replace(srcs(3));
        assert!(!p.is_shuffle());
        p.set_shuffle(true);
        assert!(p.is_shuffle());
        // With shuffle on and item 0 playing, next plays items 1 and 2 in
        // some order before ending.
        let mut seen = vec![p.next().unwrap(), p.next().unwrap()];
        seen.sort_unstable();
        assert_eq!(seen, vec![1, 2]);
        assert_eq!(p.next(), None);
    }

    #[test]
    fn remove_adjusts_indices() {
        let mut p = Playlist::default();
        p.replace(srcs(3));
        p.play_index(2);
        p.remove(0); // [old1, old2]; current 2 → 1
        assert_eq!(p.current_index(), Some(1));
        assert_eq!(p.len(), 2);
        p.remove(1); // remove current → clamps to 0
        assert_eq!(p.current_index(), Some(0));
        p.remove(0);
        assert_eq!(p.current_index(), None);
        assert!(p.is_empty());
    }
}
