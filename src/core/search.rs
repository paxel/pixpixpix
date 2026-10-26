//! The `r` search of the dir column: the shallowest folder below the column's
//! folder with images fills the band, and picking one of them moves there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{Command, Core, Focus, NavState, Screen, Thumb, screen};
use crate::decode::DecodedImage;
use crate::folder::Found;

#[derive(Debug)]
pub(super) enum Search {
    Running {
        dir: PathBuf,
        dots: bool,
    },
    NothingFound {
        dir: PathBuf,
    },
    Found {
        dir: PathBuf,
        /// The folder's first file and as many after it as the cache keeps
        /// per side.
        files: Vec<PathBuf>,
        cache: BTreeMap<PathBuf, Result<DecodedImage, String>>,
        /// Decodes waited for, asked for by the search or by the nav cache.
        decoding: BTreeSet<PathBuf>,
    },
}

impl Core {
    /// `r`: searches below the column's folder, once until the next
    /// navigation.
    pub(super) fn start_search(&mut self) -> Vec<Command> {
        let Some(column) = &self.view.dir_column else {
            return Vec::new();
        };
        if self.search.is_some() {
            return Vec::new();
        }
        let dir = column.dir.clone();
        let dots = self.dots;
        self.search = Some(Search::Running {
            dir: dir.clone(),
            dots,
        });
        vec![Command::Search { dir, dots }]
    }

    /// Drops the search, cancelling the work only it waits for.
    pub(super) fn abort_search(&mut self) -> Vec<Command> {
        match self.search.take() {
            Some(Search::Running { .. }) => vec![Command::CancelSearch],
            Some(Search::Found { decoding, .. }) => decoding
                .into_iter()
                .filter(|p| !self.decoding_elsewhere(p))
                .map(Command::CancelDecode)
                .collect(),
            Some(Search::NothingFound { .. }) | None => Vec::new(),
        }
    }

    /// Whether the current folder or the nav cache waits for the decode of
    /// `path`.
    fn decoding_elsewhere(&self, path: &Path) -> bool {
        self.decoding.contains(path)
            || self
                .nav
                .values()
                .any(|n| matches!(&n.state, NavState::Decoding(p) if p == path))
    }

    /// The result of the running search; first images the nav cache holds are
    /// taken from there.
    pub(super) fn searched(
        &mut self,
        dir: PathBuf,
        dots: bool,
        found: Option<Found>,
    ) -> Vec<Command> {
        let running = matches!(&self.search, Some(Search::Running { dir: d, dots: o }) if *d == dir && *o == dots);
        if !running {
            return Vec::new();
        }
        let Some(Found { dir: found, files }) = found else {
            self.search = Some(Search::NothingFound { dir });
            return Vec::new();
        };
        let files: Vec<PathBuf> = files
            .into_iter()
            .take(self.view.settings.cache_per_side.saturating_add(1))
            .collect();
        let cache = self
            .nav
            .values()
            .filter_map(|n| match &n.state {
                NavState::Done(path, result) if files.contains(path) => {
                    Some((path.clone(), result.clone()))
                }
                _ => None,
            })
            .collect();
        self.search = Some(Search::Found {
            dir: found,
            files,
            cache,
            decoding: BTreeSet::new(),
        });
        self.search_refresh()
    }

    /// Asks for the decode of the first file found, and of the others once it
    /// is in, as neighbours are; a decode already running elsewhere is waited
    /// for instead.
    fn search_refresh(&mut self) -> Vec<Command> {
        let Some(Search::Found {
            files,
            cache,
            decoding,
            ..
        }) = &self.search
        else {
            return Vec::new();
        };
        let first_done = files.first().is_some_and(|f| cache.contains_key(f));
        let wanted: Vec<(PathBuf, bool)> = files
            .iter()
            .take(if first_done { files.len() } else { 1 })
            .filter(|p| !cache.contains_key(*p) && !decoding.contains(*p))
            .map(|p| (p.clone(), !self.decoding_elsewhere(p)))
            .collect();
        let Some(Search::Found { decoding, .. }) = &mut self.search else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        for (path, ask) in wanted {
            decoding.insert(path.clone());
            if ask {
                commands.push(Command::Decode(path));
            }
        }
        commands
    }

    /// Keeps a decode the search waits for.
    pub(super) fn search_decoded(
        &mut self,
        path: &Path,
        result: &Result<DecodedImage, String>,
    ) -> Vec<Command> {
        let Some(Search::Found {
            cache, decoding, ..
        }) = &mut self.search
        else {
            return Vec::new();
        };
        if !decoding.remove(path) {
            return Vec::new();
        }
        cache.insert(path.to_path_buf(), result.clone());
        self.search_refresh()
    }

    /// The main area and band while the dir column decides what is shown and a
    /// search ran: its progress, or the first file found with the band holding
    /// the decoded ones.
    pub(super) fn search_view(&self) -> Option<(Screen, Vec<Thumb>)> {
        if !self.column_shown() {
            return None;
        }
        Some(match self.search.as_ref()? {
            Search::Running { dir, .. } => (Screen::Searching { dir: dir.clone() }, Vec::new()),
            Search::NothingFound { dir } => (Screen::NothingBelow { dir: dir.clone() }, Vec::new()),
            Search::Found { files, cache, .. } => {
                let first = files.first()?;
                let shown = match cache.get(first) {
                    Some(result) => screen(first, result.clone()),
                    None => Screen::Loading {
                        path: first.clone(),
                    },
                };
                let band = files
                    .iter()
                    .filter_map(|path| {
                        let image = cache.get(path)?.as_ref().ok().cloned();
                        Some(Thumb {
                            path: path.clone(),
                            image,
                            current: path == first,
                            animated: false,
                            images: 1,
                            video: false,
                        })
                    })
                    .collect();
                (shown, band)
            }
        })
    }

    /// A band file found by the search was picked: its folder becomes the
    /// current one and the image view shows the file. `None` while the band
    /// is not the search's.
    pub(super) fn show_found(&mut self, path: &Path) -> Option<Vec<Command>> {
        if !self.column_shown() {
            return None;
        }
        let Some(Search::Found { cache, .. }) = &self.search else {
            return None;
        };
        if !cache.contains_key(path) {
            return Some(Vec::new());
        }
        let Some(Search::Found {
            dir,
            mut cache,
            decoding,
            ..
        }) = self.search.take()
        else {
            return None;
        };
        let mut commands = self.enter(dir);
        if let Some(result) = cache.remove(path) {
            self.cache.insert(path.to_path_buf(), result);
        }
        commands.extend(
            decoding
                .into_iter()
                .filter(|p| !self.decoding_elsewhere(p))
                .map(Command::CancelDecode),
        );
        commands.extend(self.go_to(path.to_path_buf()));
        self.set_focus(Focus::Image);
        Some(commands)
    }
}
