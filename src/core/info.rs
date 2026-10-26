//! The metadata view of the current file and its capture date edit.

use std::path::PathBuf;

use super::{Command, Core, Screen};
use crate::metadata::{self, Field, Metadata};

/// What the metadata view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataView {
    /// Being read; nothing to show yet.
    pub reading: bool,
    /// Name, size and modification time of the file, and the size of the
    /// image shown.
    pub file: Vec<Field>,
    /// The text of the capture date field; absent without a capture date.
    pub date: Option<String>,
    /// Camera, exposure, place and the like.
    pub exif: Vec<Field>,
    /// Why the metadata cannot be read, the typed date is no date, or it
    /// could not be written.
    pub error: Option<String>,
    /// The date is being written.
    pub writing: bool,
}

/// What the typed date must look like.
pub const DATE_FORMAT_ERROR: &str = "Enter the date as YYYY-MM-DD HH:MM:SS";

/// The open metadata view.
#[derive(Debug, Default)]
pub(super) struct Info {
    /// The file it is about.
    path: Option<PathBuf>,
    /// Absent while it is read.
    read: Option<Result<Metadata, String>>,
    /// The date typed and not yet written and read back.
    draft: Option<String>,
    error: Option<String>,
    writing: bool,
}

impl Core {
    /// Opens or closes the metadata view.
    pub(super) fn toggle_info(&mut self) {
        self.info = match self.info {
            Some(_) => None,
            None => Some(Info::default()),
        };
    }

    /// Fits the open metadata view to the current file: a new file is read.
    pub(super) fn sync_info(&mut self) -> Vec<Command> {
        let current = self.current.clone();
        let Some(info) = self.info.as_mut().filter(|i| i.path != current) else {
            return Vec::new();
        };
        *info = Info {
            path: current.clone(),
            ..Info::default()
        };
        current.map(Command::ReadMetadata).into_iter().collect()
    }

    /// The metadata of `path` arrived; a date typed before it was written
    /// gives way to the one read.
    pub(super) fn metadata_read(&mut self, path: PathBuf, result: Result<Metadata, String>) {
        if let Some(info) = self.info.as_mut().filter(|i| i.path == Some(path)) {
            info.read = Some(result);
            if !info.writing {
                info.draft = None;
            }
        }
    }

    /// The capture date field changed.
    pub(super) fn date_edited(&mut self, typed: String) {
        if let Some(info) = self.info.as_mut().filter(|i| !i.writing) {
            info.draft = Some(typed);
            info.error = None;
        }
    }

    /// `Return` in the capture date field: a changed, valid date is written.
    pub(super) fn date_entered(&mut self) -> Vec<Command> {
        let Some(info) = self.info.as_mut().filter(|i| !i.writing) else {
            return Vec::new();
        };
        let (Some(path), Some(draft)) = (info.path.clone(), info.draft.clone()) else {
            return Vec::new();
        };
        let captured = match &info.read {
            Some(Ok(read)) => read.captured.as_ref(),
            _ => None,
        };
        if captured.is_none() || captured == Some(&draft) {
            return Vec::new();
        }
        let Some(date) = metadata::stored_date(&draft) else {
            info.error = Some(DATE_FORMAT_ERROR.to_owned());
            return Vec::new();
        };
        info.writing = true;
        info.error = None;
        vec![Command::WriteDate { path, date }]
    }

    /// The date was written and is read back; the pixels did not change, so
    /// the image is not decoded again. A failure keeps the typed date.
    pub(super) fn date_written(
        &mut self,
        path: PathBuf,
        result: Result<(), String>,
    ) -> Vec<Command> {
        let Some(info) = self
            .info
            .as_mut()
            .filter(|i| i.writing && i.path.as_ref() == Some(&path))
        else {
            return Vec::new();
        };
        info.writing = false;
        match result {
            Ok(()) => vec![Command::ReadMetadata(path)],
            Err(message) => {
                info.error = Some(message);
                Vec::new()
            }
        }
    }

    /// Puts the metadata view into the view; the overview hides it.
    pub(super) fn publish_info(&mut self) {
        let info = self.info.as_ref().filter(|_| self.view.overview.is_none());
        self.view.metadata = info.map(|info| {
            let read = info.read.as_ref().and_then(|r| r.as_ref().ok());
            let mut file = read.map(|r| r.file.clone()).unwrap_or_default();
            if let (Some(_), Screen::Image { path, image }) = (read, &self.screen)
                && info.path.as_ref() == Some(path)
            {
                file.push(Field {
                    label: "Image size",
                    value: format!("{} × {}", image.width, image.height),
                });
            }
            let failed = info.read.as_ref().and_then(|r| r.as_ref().err());
            MetadataView {
                reading: info.path.is_some() && info.read.is_none(),
                file,
                date: info
                    .draft
                    .clone()
                    .or_else(|| read.and_then(|r| r.captured.clone())),
                exif: read.map(|r| r.exif.clone()).unwrap_or_default(),
                error: info.error.clone().or_else(|| failed.cloned()),
                writing: info.writing,
            }
        });
    }
}
