/*
 * Copyright (c) Peter Bjorklund. All rights reserved. https://github.com/swamp/swamp
 * Licensed under the MIT License. See LICENSE in the project root for license information.
 */
use message_channel::{Channel, Receiver};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use notify::event::ModifyKind;
use thiserror::Error;
use tracing::{debug, error};
use notify::{Event, Result as NotifyResult};
#[derive(Debug)]
pub enum ChangeMessage {
    SomeKindOfChange,
}

#[derive(Error, Debug)]
pub enum FileWatcherError {
    #[error("Filesystem I/O error: {0}")]
    IoError(String),

    #[error("Watch path not found: '{0}'")]
    PathNotFound(PathBuf),

    #[error("System watcher limit reached (too many watched files/directories)")]
    TooManyWatches,

    #[error("Invalid watcher configuration: {0:?}")]
    InvalidWatcherConfig(Config),

    #[error("Generic watcher error: {0}")]
    WatcherGenericError(String),

    #[error("Internal channel communication error: {0}")]
    InternalChannelError(String),

    #[error("Attempted to remove a watch that does not exist for path: '{0}'")]
    WatchNotFound(PathBuf),
}

fn map_notify_error_to_file_watcher_error(e: notify::Error, path: &Path) -> FileWatcherError {
    use notify::ErrorKind;
    match e.kind {
        ErrorKind::PathNotFound => FileWatcherError::PathNotFound(path.to_path_buf()),
        ErrorKind::MaxFilesWatch => FileWatcherError::TooManyWatches,
        ErrorKind::Generic(msg) => FileWatcherError::WatcherGenericError(msg),
        ErrorKind::InvalidConfig(config) => FileWatcherError::InvalidWatcherConfig(config),
        ErrorKind::WatchNotFound => FileWatcherError::WatchNotFound(path.to_path_buf()),

        ErrorKind::Io(io_err) => FileWatcherError::IoError(io_err.to_string()),
    }
}

#[derive(Debug)]
pub struct FileWatcher {
    pub receiver: Receiver<ChangeMessage>,
    pub watcher: RecommendedWatcher, // keeps watcher alive
}

impl FileWatcher {
    /// # Errors
    ///
    pub fn new(watch_path: &Path) -> Result<Self, FileWatcherError> {
        let (watcher, receiver) = start_watch(watch_path)?;
        while let Ok(_found) = receiver.recv() {
        }
        Ok(Self { receiver, watcher })
    }

    #[must_use]
    pub fn has_changed(&self) -> bool {
        let mut result = false;
        while let Ok(_found) = self.receiver.recv() {
            result = true;
        }

        result
    }
}

/// # Errors
///
/// # Panics
///
///
pub fn start_watch(
    watch_path: &Path,
) -> Result<(RecommendedWatcher, Receiver<ChangeMessage>), FileWatcherError> {
    let (sender, receiver) = Channel::create();

    let mut last_event = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let debounce_duration = Duration::from_millis(100);

    let owned_watch_path = watch_path.to_path_buf();

    let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event> | match res {
        Ok(event) if matches!(event.kind,
            EventKind::Modify(ModifyKind::Data(_))
          | EventKind::Modify(ModifyKind::Any)
          ) =>
            {
                let now = Instant::now();
                if now.duration_since(last_event) >= debounce_duration {
                    if let Err(e) = sender.send(ChangeMessage::SomeKindOfChange) {
                        error!(
                        error = ?e,
                        "FileWatcher internal channel send error: receiver likely dropped"
                    );
                    }
                    last_event = now;
                }
            }
        Ok(_) => {
            // ignore metadata, attrib, open, etc.
        }

        Err(e) => {
            error!(
                error = ?e,
                path = ?owned_watch_path,
                "FileWatcher internal background watch error"
            );
        }
    })
    .map_err(|e| {
        error!(error = ?e, path = ?watch_path, "Failed to initialize watcher");
        map_notify_error_to_file_watcher_error(e, watch_path)
    })?;

    watcher
        .watch(watch_path, RecursiveMode::Recursive)
        .map_err(|e| {
            error!(error = ?e, path = ?watch_path, "Failed to start watching path");
            map_notify_error_to_file_watcher_error(e, watch_path)
        })?;

    debug!(path = ?watch_path, "Successfully started file watcher");

    Ok((watcher, receiver))
}
