use crate::app_state::AppState;
use crate::config::RuntimeConfig;
use crate::error::WebBootstrapError;
use crate::service::{ProjectApi, SessionApi, TaskApi, WorktreeApi};
use ora_application::Clock;
use ora_db::{DatabaseBootstrapper, DatabaseLocation, RepositoryPool, default_migration_catalog};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds the application state used by the web runtime from SQLite-backed dependencies.
pub fn build_app_state(runtime_config: &RuntimeConfig) -> Result<AppState, WebBootstrapError> {
    build_app_state_for_database(runtime_config.database().path())
}

/// Builds the application state used by tests and the runtime from one database path.
pub(crate) fn build_app_state_for_database(
    database_path: &Path,
) -> Result<AppState, WebBootstrapError> {
    let pool = build_repository_pool(database_path)?;
    let clock = SystemClock;

    Ok(AppState::new(
        Arc::new(ProjectApi::new(pool.clone(), clock)),
        Arc::new(TaskApi::new(pool.clone(), clock)),
        Arc::new(WorktreeApi::new(pool.clone(), clock)),
        Arc::new(SessionApi::new(pool, clock)),
    ))
}

/// Opens the configured file-backed SQLite database and returns the shared repository pool.
fn build_repository_pool(database_path: &Path) -> Result<RepositoryPool, WebBootstrapError> {
    let catalog = default_migration_catalog().map_err(WebBootstrapError::DatabaseBootstrap)?;

    DatabaseBootstrapper::system()
        .bootstrap_repository_pool(&DatabaseLocation::path(database_path), &catalog)
        .map_err(WebBootstrapError::DatabaseBootstrap)
}

/// Reads the current wall-clock time for audit fields in the runtime.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    /// Returns the current Unix timestamp in milliseconds for handler audit fields.
    fn now_timestamp_millis(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis() as i64,
            Err(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_app_state_for_database;
    use crate::error::WebBootstrapError;
    use tempfile::TempDir;

    /// Verifies bootstrap fails cleanly when the configured database path points to a directory.
    #[test]
    fn rejects_directory_database_paths() {
        let temp_dir = TempDir::new().unwrap();
        let error = match build_app_state_for_database(temp_dir.path()) {
            Ok(_) => panic!("expected directory database path to fail"),
            Err(error) => error,
        };

        assert!(matches!(error, WebBootstrapError::DatabaseBootstrap(_)));
    }
}
