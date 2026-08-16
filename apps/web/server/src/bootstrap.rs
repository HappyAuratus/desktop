use crate::app_state::AppState;
#[cfg(test)]
use crate::config::RuntimeBinaryPaths;
use crate::config::RuntimeConfig;
use crate::error::WebBootstrapError;
use crate::service::{FileSystemApi, WorkspaceFileApi};
use ora_backend::{Backend, BackendBootstrapError, BackendPaths};
use ora_logging::ora_warn;
use ora_plugin_manager::PluginManager;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Builds the application state used by the web runtime from SQLite-backed dependencies.
pub fn build_app_state(runtime_config: &RuntimeConfig) -> Result<AppState, WebBootstrapError> {
    let backend = build_backend(
        runtime_config.database().path(),
        runtime_config.worktree().root(),
        runtime_config.file_system().home_directory(),
        runtime_config.history().sessions_root(),
        runtime_config.binaries().ripgrep_path(),
        runtime_config.logging().timezone,
    )?;
    let data_dir = runtime_config
        .database()
        .path()
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let plugin_manager = discover_plugins(data_dir);

    Ok(AppState::new(
        backend,
        Arc::new(FileSystemApi::new(
            runtime_config.file_system().home_directory().to_path_buf(),
        )),
        Arc::new(WorkspaceFileApi::new(
            runtime_config.binaries().ripgrep_path().to_path_buf(),
        )),
        Arc::new(plugin_manager),
        runtime_config.binaries().clone(),
    ))
}

/// Builds application state for tests from explicit filesystem paths.
#[cfg(test)]
pub(crate) fn build_app_state_for_database(
    database_path: &Path,
    project_root: &Path,
    work_dir: &Path,
    data_dir: &Path,
) -> Result<AppState, WebBootstrapError> {
    let test_executable = std::env::current_exe()
        .unwrap_or_else(|error| panic!("expected current test executable path: {error}"));
    let binary_paths = RuntimeBinaryPaths::for_tests(Path::new("rg"), &test_executable);
    let backend = build_backend(
        database_path,
        work_dir,
        project_root.parent().unwrap_or(project_root),
        &work_dir.with_file_name("sessions"),
        binary_paths.ripgrep_path(),
        chrono_tz::UTC,
    )?;

    Ok(AppState::new(
        backend,
        Arc::new(FileSystemApi::new(
            project_root.parent().unwrap_or(project_root).to_path_buf(),
        )),
        Arc::new(WorkspaceFileApi::new(
            binary_paths.ripgrep_path().to_path_buf(),
        )),
        Arc::new(discover_plugins(data_dir)),
        binary_paths,
    ))
}

/// Captures one startup snapshot and reports every isolated package problem.
fn discover_plugins(data_dir: &Path) -> PluginManager {
    let manager = PluginManager::discover(data_dir);
    for issue in manager.discovery_issues() {
        ora_warn!(
            message = "installed plugin manifest skipped during discovery",
            path = %issue.path().display(),
            issue_kind = issue.kind().as_str(),
            field_path = issue.field_path().unwrap_or(""),
            reason = issue.message(),
        );
    }
    manager
}

/// Opens the shared backend while preserving the server's existing bootstrap error variants.
fn build_backend(
    database_path: &Path,
    worktree_root: &Path,
    home_directory: &Path,
    sessions_root: &Path,
    ripgrep_path: &Path,
    timezone: chrono_tz::Tz,
) -> Result<Backend, WebBootstrapError> {
    Backend::open(BackendPaths {
        database_path: database_path.to_path_buf(),
        worktree_root: worktree_root.to_path_buf(),
        home_directory: home_directory.to_path_buf(),
        relative_path_base: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        sessions_root: sessions_root.to_path_buf(),
        skills_root: database_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("atoms")
            .join("skills"),
        ripgrep_path: ripgrep_path.to_path_buf(),
        timezone,
    })
    .map_err(web_backend_bootstrap_error)
}

/// Maps shared backend bootstrap failures into the stable Web process error surface.
fn web_backend_bootstrap_error(error: BackendBootstrapError) -> WebBootstrapError {
    match error {
        BackendBootstrapError::DirectoryCreate { source, .. } => {
            WebBootstrapError::DataDirectoryCreate(source)
        }
        BackendBootstrapError::Database(source) => WebBootstrapError::DatabaseBootstrap(source),
        BackendBootstrapError::SkillStorage(source) => {
            WebBootstrapError::SkillStorageReconcile { source }
        }
        BackendBootstrapError::AgentRuntime(source) => {
            WebBootstrapError::BackendRuntimeBootstrap(source)
        }
        BackendBootstrapError::SkillStorageReconciliation(source) => {
            WebBootstrapError::SkillStorageReconciliation(source)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_app_state, build_app_state_for_database};
    use crate::config::RuntimeConfig;
    use crate::error::WebBootstrapError;
    use ora_application::ProjectRepository;
    use ora_db::{
        DatabaseBootstrapper, DatabaseLocation, SqliteProjectRepository, default_migration_catalog,
    };
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use tempfile::TempDir;

    /// Verifies bootstrap fails cleanly when the configured database path points to a directory.
    #[test]
    fn rejects_directory_database_paths() {
        let temp_dir = TempDir::new().unwrap();
        let error = match build_app_state_for_database(
            temp_dir.path(),
            temp_dir.path(),
            &temp_dir.path().join("worktrees"),
            temp_dir.path(),
        ) {
            Ok(_) => panic!("expected directory database path to fail"),
            Err(error) => error,
        };

        assert!(matches!(error, WebBootstrapError::DatabaseBootstrap(_)));
    }

    /// Verifies runtime bootstrap becomes usable without creating a project.
    #[test]
    fn starts_with_an_empty_project_catalog() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("empty-bootstrap");
        let runtime_config = runtime_config(&data_dir);
        let database_path = data_dir.join("ora.sqlite3");

        build_app_state(&runtime_config)
            .unwrap_or_else(|error| panic!("expected runtime bootstrap to succeed: {error}"));

        let repository = bootstrapped_project_repository(&database_path);

        assert_eq!(repository.list_projects().unwrap(), Vec::new());
    }

    /// Builds one runtime configuration without mutating process environment during tests.
    fn runtime_config(data_dir: &Path) -> RuntimeConfig {
        let binary_path = std::env::current_exe().unwrap();
        RuntimeConfig::from_reader(|key| match key {
            "ORA_DATA_DIR" => Some(data_dir.to_string_lossy().to_string()),
            "HOME" => Some(data_dir.to_string_lossy().to_string()),
            "ORA_RG_PATH" | "ORA_DENO_PATH" => Some(binary_path.to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected runtime configuration to load: {error}"))
    }

    /// Opens the test database so bootstrap assertions can inspect persisted project state.
    fn bootstrapped_project_repository(database_path: &Path) -> SqliteProjectRepository {
        let pool = DatabaseBootstrapper::system()
            .bootstrap_repository_pool(
                &DatabaseLocation::path(database_path),
                &default_migration_catalog().unwrap(),
            )
            .unwrap_or_else(|error| {
                panic!("expected repository pool bootstrap to succeed: {error}")
            });

        SqliteProjectRepository::new(pool)
    }
}
