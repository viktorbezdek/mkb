//! File system watcher for auto-reindexing vault changes.
//!
//! Uses the `notify` crate for cross-platform file system events
//! (FSEvents on macOS, inotify on Linux, ReadDirectoryChanges on Windows).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use mkb_core::error::MkbError;

/// Events emitted by the vault watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultEvent {
    /// A markdown file was created or modified.
    Changed(PathBuf),
    /// A markdown file was deleted.
    Removed(PathBuf),
}

/// Watches a vault directory for file changes and emits events.
pub struct VaultWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<VaultEvent>,
}

impl VaultWatcher {
    /// Start watching a vault directory for changes.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if the watcher cannot be created.
    pub fn start(vault_root: &Path) -> Result<Self, MkbError> {
        let (tx, rx) = mpsc::channel();

        let tx_clone = tx.clone();
        let vault_root_owned = vault_root.to_path_buf();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                for path in &event.paths {
                    // Skip non-markdown files
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    // Skip hidden directories (.mkb, .archive)
                    if path
                        .strip_prefix(&vault_root_owned)
                        .ok()
                        .and_then(|rel| rel.components().next())
                        .and_then(|c| c.as_os_str().to_str())
                        .is_some_and(|s| s.starts_with('.'))
                    {
                        continue;
                    }

                    let vault_event = match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            VaultEvent::Changed(path.clone())
                        }
                        EventKind::Remove(_) => VaultEvent::Removed(path.clone()),
                        _ => continue,
                    };
                    let _ = tx_clone.send(vault_event);
                }
            }
        })
        .map_err(|e| MkbError::Io(std::io::Error::other(e)))?;

        watcher
            .watch(vault_root, RecursiveMode::Recursive)
            .map_err(|e| MkbError::Io(std::io::Error::other(e)))?;

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    /// Try to receive the next event with a timeout.
    ///
    /// Returns `None` if no event is available within the timeout.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<VaultEvent> {
        self.receiver.recv_timeout(timeout).ok()
    }

    /// Try to receive the next event without blocking.
    ///
    /// Returns `None` if no event is immediately available.
    pub fn try_recv(&self) -> Option<VaultEvent> {
        self.receiver.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn watcher_detects_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path();
        fs::create_dir_all(vault_root.join(".mkb")).unwrap();
        fs::create_dir_all(vault_root.join("projects")).unwrap();

        let watcher = VaultWatcher::start(vault_root).unwrap();

        // Create a new markdown file
        let file = vault_root.join("projects").join("test.md");
        fs::write(&file, "# Test\nContent").unwrap();

        // Wait for event (up to 2s)
        let event = watcher.recv_timeout(Duration::from_secs(2));
        assert!(
            event.is_some(),
            "Expected watcher to detect new file creation"
        );
        match event.unwrap() {
            VaultEvent::Changed(path) => {
                assert!(path.to_string_lossy().contains("test.md"));
            }
            other => panic!("Expected Changed event, got {other:?}"),
        }
    }

    #[test]
    fn watcher_detects_modification() {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path();
        fs::create_dir_all(vault_root.join(".mkb")).unwrap();
        fs::create_dir_all(vault_root.join("projects")).unwrap();

        let file = vault_root.join("projects").join("existing.md");
        fs::write(&file, "# Existing\nOriginal").unwrap();

        let watcher = VaultWatcher::start(vault_root).unwrap();

        // Modify the file
        std::thread::sleep(Duration::from_millis(100));
        fs::write(&file, "# Existing\nModified").unwrap();

        let event = watcher.recv_timeout(Duration::from_secs(2));
        assert!(
            event.is_some(),
            "Expected watcher to detect file modification"
        );
        match event.unwrap() {
            VaultEvent::Changed(path) => {
                assert!(path.to_string_lossy().contains("existing.md"));
            }
            other => panic!("Expected Changed event, got {other:?}"),
        }
    }

    #[test]
    fn watcher_detects_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path();
        fs::create_dir_all(vault_root.join(".mkb")).unwrap();
        fs::create_dir_all(vault_root.join("projects")).unwrap();

        let file = vault_root.join("projects").join("to-delete.md");
        fs::write(&file, "# Delete Me").unwrap();

        let watcher = VaultWatcher::start(vault_root).unwrap();

        std::thread::sleep(Duration::from_millis(100));
        fs::remove_file(&file).unwrap();

        // On macOS, FSEvents may emit Changed before Removed.
        // Drain events until we find a Removed event.
        let mut found_removed = false;
        for _ in 0..10 {
            match watcher.recv_timeout(Duration::from_secs(2)) {
                Some(VaultEvent::Removed(path)) => {
                    assert!(path.to_string_lossy().contains("to-delete.md"));
                    found_removed = true;
                    break;
                }
                Some(VaultEvent::Changed(_)) => continue,
                None => break,
            }
        }
        assert!(found_removed, "Expected watcher to emit Removed event");
    }

    #[test]
    fn watcher_ignores_non_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path();
        fs::create_dir_all(vault_root.join(".mkb")).unwrap();
        fs::create_dir_all(vault_root.join("projects")).unwrap();

        let watcher = VaultWatcher::start(vault_root).unwrap();

        // Create a non-markdown file
        let file = vault_root.join("projects").join("notes.txt");
        fs::write(&file, "plain text").unwrap();

        let event = watcher.recv_timeout(Duration::from_millis(500));
        assert!(event.is_none(), "Watcher should ignore non-markdown files");
    }
}
