//! The play queue. Fix X03: opening one file queues the rest of its folder in
//! natural name order ("Lesson 2" before "Lesson 10"), so courses and series
//! play in sequence.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayMode {
    /// Play the current file, then stop.
    Single,
    /// Play the list once, in order.
    #[default]
    Order,
    /// Repeat the whole list.
    LoopAll,
    /// Repeat the current file.
    LoopOne,
    /// Random order.
    Random,
}

#[derive(Debug, Default, Clone)]
pub struct Playlist {
    items: Vec<PathBuf>,
    current: Option<usize>,
    rng: u64,
}

impl Playlist {
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    pub fn current_path(&self) -> Option<&Path> {
        self.current.and_then(|i| self.items.get(i)).map(PathBuf::as_path)
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Replaces the list with `file` and its media siblings in natural order, current = `file`.
    pub fn open_with_folder(&mut self, file: &Path) {
        let mut items = file.parent().map(scan_folder).unwrap_or_default();
        if !items.iter().any(|p| same(p, file)) {
            items.push(file.to_path_buf());
            items.sort_by(|a, b| natural_cmp_path(a, b));
        }
        self.current = items.iter().position(|p| same(p, file));
        self.items = items;
    }

    /// Replaces the list with every media file in `dir`; current = first.
    pub fn open_folder(&mut self, dir: &Path) {
        self.items = scan_folder(dir);
        self.current = (!self.items.is_empty()).then_some(0);
    }

    /// Appends files, keeping the current item. Returns the index of the first added file.
    pub fn add(&mut self, files: impl IntoIterator<Item = PathBuf>) -> Option<usize> {
        let start = self.items.len();
        for f in files {
            if !self.items.iter().any(|p| same(p, &f)) {
                self.items.push(f);
            }
        }
        (self.items.len() > start).then_some(start)
    }

    pub fn select(&mut self, index: usize) -> Option<&Path> {
        if index < self.items.len() {
            self.current = Some(index);
        }
        self.current_path()
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        self.current = match self.current {
            Some(c) if c == index => None,
            Some(c) if c > index => Some(c - 1),
            c => c,
        };
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
    }

    pub fn sort_by_name(&mut self) {
        let cur = self.current_path().map(Path::to_path_buf);
        self.items.sort_by(|a, b| natural_cmp_path(a, b));
        self.current = cur.and_then(|c| self.items.iter().position(|p| *p == c));
    }

    /// The item to play after the current one finishes, per `mode`. `None` means stop.
    pub fn next_after_end(&mut self, mode: PlayMode) -> Option<usize> {
        let n = self.items.len();
        let cur = self.current?;
        match mode {
            PlayMode::Single => None,
            PlayMode::LoopOne => Some(cur),
            PlayMode::Order => (cur + 1 < n).then_some(cur + 1),
            PlayMode::LoopAll => (n > 0).then_some((cur + 1) % n),
            PlayMode::Random => (n > 1).then(|| self.random_other(cur)),
        }
    }

    /// The next item for the Next button: wraps around, honours Random.
    pub fn next(&mut self, mode: PlayMode) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let cur = self.current.unwrap_or(n - 1);
        Some(if mode == PlayMode::Random && n > 1 { self.random_other(cur) } else { (cur + 1) % n })
    }

    pub fn previous(&mut self) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let cur = self.current.unwrap_or(0);
        Some((cur + n - 1) % n)
    }

    fn random_other(&mut self, cur: usize) -> usize {
        // xorshift seeded from the clock; good enough for shuffling a playlist.
        if self.rng == 0 {
            self.rng = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9E37_79B9) | 1;
        }
        let n = self.items.len();
        loop {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            let i = (self.rng % n as u64) as usize;
            if i != cur {
                return i;
            }
        }
    }
}

fn same(a: &Path, b: &Path) -> bool {
    a == b || a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Media files directly in `dir`, in natural name order.
pub fn scan_folder(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_file() && crate::is_media(p)).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| natural_cmp_path(a, b));
    v
}

fn natural_cmp_path(a: &Path, b: &Path) -> Ordering {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    natural_cmp(&name(a), &name(b))
}

/// Compares strings so that runs of digits compare by value: "a2" < "a10".
/// Letters compare case-insensitively.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(&ch) = it.peek() {
                        if !ch.is_ascii_digit() {
                            break;
                        }
                        s.push(ch);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut x), take(&mut y));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(c), Some(d)) => {
                let ord = c.to_lowercase().cmp(d.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["Lesson 10.mp4", "lesson 2.mp4", "Lesson 1.mp4", "Lesson 02b.mp4", "درس 3.mp4"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Lesson 1.mp4", "lesson 2.mp4", "Lesson 02b.mp4", "Lesson 10.mp4", "درس 3.mp4"]);
    }

    #[test]
    fn end_of_file_modes() {
        let mut p = Playlist::default();
        p.add(["a", "b", "c"].map(PathBuf::from));
        p.select(2);
        assert_eq!(p.next_after_end(PlayMode::Order), None);
        assert_eq!(p.next_after_end(PlayMode::LoopAll), Some(0));
        assert_eq!(p.next_after_end(PlayMode::LoopOne), Some(2));
        assert_eq!(p.next_after_end(PlayMode::Single), None);
        let r = p.next_after_end(PlayMode::Random).unwrap();
        assert!(r < 3 && r != 2);
    }

    #[test]
    fn remove_keeps_current() {
        let mut p = Playlist::default();
        p.add(["a", "b", "c"].map(PathBuf::from));
        p.select(2);
        p.remove(0);
        assert_eq!(p.current(), Some(1));
        p.remove(1);
        assert_eq!(p.current(), None);
    }
}
