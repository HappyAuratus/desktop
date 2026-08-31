use ora_logging::{ora_error, ora_info};

use crate::{
    DatabaseError, DatabaseLocation, MigrationCatalog, RepositoryPool, SystemTimestampSource,
    TimestampSource, migration,
};

/// Coordinates opening SQLite connections and reconciling them with the migration catalog.
#[derive(Debug)]
pub struct DatabaseBootstrapper<T> {
    timestamp_source: T,
}

impl DatabaseBootstrapper<SystemTimestampSource> {
    /// Builds a bootstrapper that timestamps applied migrations from the system clock.
    pub fn system() -> Self {
        Self::new(SystemTimestampSource)
    }
}

impl<T> DatabaseBootstrapper<T>
where
    T: TimestampSource,
{
    /// Builds a bootstrapper around a caller-provided timestamp source for deterministic tests.
    pub fn new(timestamp_source: T) -> Self {
        Self { timestamp_source }
    }

    /// Opens a repository pool and reconciles its database with the target migration prefix.
    pub fn bootstrap_repository_pool(
        &self,
        location: &DatabaseLocation,
        catalog: &MigrationCatalog,
    ) -> Result<RepositoryPool, DatabaseError> {
        ora_info!(message = "opening database", operation = "database_open");

        let pool = match RepositoryPool::new(location) {
            Ok(pool) => pool,
            Err(error) => {
                ora_error!(
                    message = "failed to open database",
                    operation = "database_open",
                    error.kind = "database_open",
                    error.message = error.to_string()
                );
                return Err(error);
            }
        };

        ora_info!(message = "opened database", operation = "database_open");

        if let Err(error) = pool.with_connection_mut(|connection| {
            migration::reconcile_database(connection, catalog, &self.timestamp_source)
        }) {
            ora_error!(
                message = "database bootstrap failed",
                operation = "database_bootstrap",
                error.kind = "database_bootstrap",
                error.message = error.to_string()
            );
            return Err(error);
        }

        ora_info!(
            message = "database bootstrap complete",
            operation = "database_bootstrap",
        );

        Ok(pool)
    }
}
