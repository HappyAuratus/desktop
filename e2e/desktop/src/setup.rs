//! Shared setup and teardown for isolated Desktop E2E cases.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ora_backend::BackendPaths;
use tempfile::{TempDir, tempdir};

/// Owns the filesystem sandbox and Backend paths for one Desktop E2E case.
pub(crate) struct DesktopTestSetup {
    directory: TempDir,
    paths: BackendPaths,
}

impl DesktopTestSetup {
    /// Creates one isolated filesystem sandbox using the production file-backed database layout.
    pub(crate) fn new() -> io::Result<Self> {
        let directory = tempdir()?;
        let root = directory.path().to_path_buf();
        let app_data_directory = root.join("app_data");
        let home_directory = root.join("home");
        fs::create_dir_all(&app_data_directory)?;
        fs::create_dir_all(&home_directory)?;

        Ok(Self {
            paths: BackendPaths {
                app_data_directory,
                home_directory,
                deno_path: PathBuf::from("deno"),
                relative_path_base: root,
                timezone: chrono_tz::Asia::Shanghai,
            },
            directory,
        })
    }

    /// Returns the root that contains every file owned by this E2E case.
    pub(crate) fn root(&self) -> &Path {
        self.directory.path()
    }

    /// Returns the production-shaped paths assigned to this E2E case.
    pub(crate) fn backend_paths(&self) -> &BackendPaths {
        &self.paths
    }
}
