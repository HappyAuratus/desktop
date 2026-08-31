//! Integration coverage for Workspace-scoped Effect convergence.

#[cfg(test)]
mod tests {
    use crate::setup::DesktopTestSetup;
    use ora_db::{DatabaseBootstrapper, DatabaseLocation, default_migration_catalog};
    use pretty_assertions::assert_eq;

    /// Verifies one E2E case uses isolated Backend paths and a file-backed database.
    #[test]
    fn uses_isolated_file_backed_backend_paths() -> Result<(), Box<dyn std::error::Error>> {
        let setup = DesktopTestSetup::new()?;
        let root = setup.root().to_path_buf();
        let paths = setup.backend_paths();
        let database_path = paths.app_data_directory.join("ora.sqlite3");
        let catalog = default_migration_catalog()?;
        let _pool = DatabaseBootstrapper::system()
            .bootstrap_repository_pool(&DatabaseLocation::path(&database_path), &catalog)?;
        let expected_app_data_directory = root.join("app_data");
        let expected_home_directory = root.join("home");

        assert_eq!(
            (
                &paths.app_data_directory,
                &paths.home_directory,
                database_path.is_file(),
            ),
            (&expected_app_data_directory, &expected_home_directory, true,),
        );
        Ok(())
    }
}
