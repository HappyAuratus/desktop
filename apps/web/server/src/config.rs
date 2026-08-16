use crate::error::WebBootstrapError;
use crate::timezone::{TimezoneSource, TimezoneWarning};
use ora_logging::{FileLoggingConfig, LogLevel, LogOutput, LoggingConfig, RotationPolicy};
use std::env;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

const DATA_DIR_ENV_VAR: &str = "ORA_DATA_DIR";
const WORKTREE_DIR_ENV_VAR: &str = "ORA_WORKTREE_DIR";
const HOST_ENV_VAR: &str = "ORA_HOST";
const PORT_ENV_VAR: &str = "ORA_PORT";
const LOG_LEVEL_ENV_VAR: &str = "ORA_LOG_LEVEL";
const LOG_MODE_ENV_VAR: &str = "ORA_LOG_MODE";
const LOG_MAX_DAYS_ENV_VAR: &str = "ORA_LOG_MAX_DAYS";
const RIPGREP_PATH_ENV_VAR: &str = "ORA_RG_PATH";
const DENO_PATH_ENV_VAR: &str = "ORA_DENO_PATH";
// Shared with the `timezone` module, which owns the resolution logic that consumes them.
pub(crate) const TIMEZONE_ENV_VAR: &str = "ORA_TIMEZONE";
pub(crate) const SYSTEM_TIMEZONE_ENV_VAR: &str = "TZ";
const HOME_ENV_VAR: &str = "HOME";
const USER_PROFILE_ENV_VAR: &str = "USERPROFILE";

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 32578;
const DEFAULT_LOG_LEVEL: &str = "info";
const DEFAULT_LOG_MODE: &str = "stdout";
const DEFAULT_LOG_MAX_DAYS: &str = "3";
pub(crate) const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";

/// Groups the runtime configuration required to bootstrap the web server process.
pub struct RuntimeConfig {
    binaries: RuntimeBinaryPaths,
    database: DatabaseConfig,
    history: HistoryConfig,
    file_system: FileSystemConfig,
    worktree: WorktreeConfig,
    server: ServerConfig,
    logging: LoggingConfig,
    timezone_source: TimezoneSource,
    timezone_warning: Option<TimezoneWarning>,
}

impl RuntimeConfig {
    /// Loads the runtime configuration from the environment-backed server contract.
    pub fn from_env() -> Result<Self, WebBootstrapError> {
        Self::from_reader(|key| env::var(key).ok())
    }

    /// Returns the database configuration used by the runtime bootstrap.
    pub fn database(&self) -> &DatabaseConfig {
        &self.database
    }

    /// Returns the explicit executable paths used by the Web runtime.
    pub fn binaries(&self) -> &RuntimeBinaryPaths {
        &self.binaries
    }

    /// Returns where Ora-owned session history is stored.
    pub fn history(&self) -> &HistoryConfig {
        &self.history
    }

    /// Returns the filesystem root used for task-owned linked worktrees.
    pub fn worktree(&self) -> &WorktreeConfig {
        &self.worktree
    }

    /// Returns the filesystem configuration used by server-side path browsing.
    pub fn file_system(&self) -> &FileSystemConfig {
        &self.file_system
    }

    /// Returns the server bind configuration used by the runtime.
    pub fn server(&self) -> &ServerConfig {
        &self.server
    }

    /// Returns the shared logging configuration used during process bootstrap.
    pub fn logging(&self) -> &LoggingConfig {
        &self.logging
    }

    /// Returns where the Web process obtained its selected logging timezone.
    pub(crate) fn timezone_source(&self) -> TimezoneSource {
        self.timezone_source
    }

    /// Returns the deferred timezone warning to emit after logging becomes available.
    pub(crate) fn timezone_warning(&self) -> Option<&TimezoneWarning> {
        self.timezone_warning.as_ref()
    }

    /// Loads the runtime configuration from a caller-provided variable reader for testability.
    pub(crate) fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        let database = DatabaseConfig::from_reader(&mut read_variable)?;
        let file_system = FileSystemConfig::from_reader(&mut read_variable)?;
        let worktree = WorktreeConfig::from_reader(&mut read_variable, &file_system)?;
        let resolved_timezone = crate::timezone::resolve(&mut read_variable);

        Ok(Self {
            binaries: RuntimeBinaryPaths::from_reader(&mut read_variable)?,
            worktree,
            history: HistoryConfig::from_reader(&mut read_variable)?,
            file_system,
            database,
            server: ServerConfig::from_reader(&mut read_variable)?,
            logging: read_logging_config(&mut read_variable, resolved_timezone.timezone)?,
            timezone_source: resolved_timezone.source,
            timezone_warning: resolved_timezone.warning,
        })
    }
}

/// Stores the required external executables configured for the Web runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBinaryPaths {
    ripgrep: PathBuf,
    deno: PathBuf,
}

impl RuntimeBinaryPaths {
    /// Returns the explicit ripgrep executable consumed by Backend and ora-fs.
    pub fn ripgrep_path(&self) -> &Path {
        self.ripgrep.as_path()
    }

    /// Returns the explicit Deno executable reserved for Rust-owned integrations.
    pub fn deno_path(&self) -> &Path {
        self.deno.as_path()
    }

    /// Loads both required executable paths from the environment-backed reader.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        Ok(Self {
            ripgrep: read_binary_path(RIPGREP_PATH_ENV_VAR, &mut read_variable)?,
            deno: read_binary_path(DENO_PATH_ENV_VAR, &mut read_variable)?,
        })
    }

    /// Creates explicit binary paths for bootstrap tests without mutating process environment.
    #[cfg(test)]
    pub(crate) fn for_tests(ripgrep_path: &Path, deno_path: &Path) -> Self {
        Self {
            ripgrep: ripgrep_path.to_path_buf(),
            deno: deno_path.to_path_buf(),
        }
    }
}

/// Reads and validates one required absolute executable path.
fn read_binary_path(
    variable: &'static str,
    mut read_variable: impl FnMut(&str) -> Option<String>,
) -> Result<PathBuf, WebBootstrapError> {
    let raw_path =
        read_variable(variable).ok_or(WebBootstrapError::BinaryPathMissing { variable })?;
    if raw_path.trim().is_empty() {
        return Err(WebBootstrapError::BinaryPathEmpty { variable });
    }
    let path = PathBuf::from(raw_path);
    if !path.is_absolute() {
        return Err(WebBootstrapError::BinaryPathNotAbsolute { variable, path });
    }
    if !path.is_file() {
        return Err(WebBootstrapError::BinaryPathNotFile { variable, path });
    }
    Ok(path)
}

/// Describes the server user's home directory used as the browser's default location.
pub struct FileSystemConfig {
    home_directory: PathBuf,
}

impl FileSystemConfig {
    /// Returns the absolute home directory used when a listing request omits its path.
    pub fn home_directory(&self) -> &Path {
        self.home_directory.as_path()
    }

    /// Resolves the conventional Unix or Windows home environment variable without mutating tests.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        let raw_home = read_variable(HOME_ENV_VAR)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                read_variable(USER_PROFILE_ENV_VAR).filter(|value| !value.trim().is_empty())
            })
            .ok_or(WebBootstrapError::HomeDirectoryUnavailable)?;
        let home_directory = PathBuf::from(raw_home);

        if !home_directory.is_absolute() {
            return Err(WebBootstrapError::HomeDirectoryNotAbsolute { home_directory });
        }

        Ok(Self { home_directory })
    }
}

/// Describes the file-backed SQLite database location used by the web runtime.
pub struct DatabaseConfig {
    path: PathBuf,
}

impl DatabaseConfig {
    /// Returns the configured SQLite database path.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Loads the database path from a caller-provided variable reader for testability.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        let data_dir = read_data_dir_root(&mut read_variable)?;

        Ok(Self {
            path: data_dir.join("ora.sqlite3"),
        })
    }
}

/// Describes where the web runtime keeps Ora-owned session history.
pub struct HistoryConfig {
    sessions_root: PathBuf,
}

impl HistoryConfig {
    /// Returns the root of the session history tree.
    pub fn sessions_root(&self) -> &Path {
        self.sessions_root.as_path()
    }

    /// Derives the history root from the same data directory every other path uses.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        let data_dir = read_data_dir_root(&mut read_variable)?;

        Ok(Self {
            sessions_root: data_dir.join("sessions"),
        })
    }
}

/// Describes the global filesystem root used for task-owned linked worktrees.
pub struct WorktreeConfig {
    root: PathBuf,
}

impl WorktreeConfig {
    /// Returns the configured linked-worktree root used for task-owned worktree provisioning.
    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    /// Loads an explicit worktree root or derives it from the server user's home directory.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
        file_system: &FileSystemConfig,
    ) -> Result<Self, WebBootstrapError> {
        let Some(raw_root) = read_variable(WORKTREE_DIR_ENV_VAR) else {
            return Ok(Self {
                root: file_system.home_directory().join(".ora").join("worktrees"),
            });
        };
        if raw_root.trim().is_empty() {
            return Err(WebBootstrapError::InvalidWorktreePathEmpty);
        }

        let root = PathBuf::from(raw_root);
        if !root.is_absolute() {
            return Err(WebBootstrapError::WorktreeDirectoryNotAbsolute {
                worktree_directory: root,
            });
        }

        Ok(Self { root })
    }
}

/// Resolves the single runtime data directory root used to derive all file paths.
///
/// Always returns an absolute path so downstream consumers (e.g. git commands that run with a
/// different working directory) resolve paths correctly regardless of the caller's cwd.
fn read_data_dir_root(
    mut read_variable: impl FnMut(&str) -> Option<String>,
) -> Result<PathBuf, WebBootstrapError> {
    let raw_data_dir = read_variable(DATA_DIR_ENV_VAR).unwrap_or_else(|| ".".to_string());

    if raw_data_dir.trim().is_empty() {
        return Err(WebBootstrapError::InvalidDatabasePathEmpty);
    }

    let path = PathBuf::from(raw_data_dir);
    if path.is_absolute() {
        return Ok(path);
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(WebBootstrapError::CurrentDirectory)
}

/// Describes the host and port that the HTTP server binds to.
pub struct ServerConfig {
    host: IpAddr,
    port: u16,
}

impl ServerConfig {
    /// Returns the bind host used by the HTTP listener.
    pub fn host(&self) -> IpAddr {
        self.host
    }

    /// Returns the bind port used by the HTTP listener.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Combines the configured host and port into the socket address consumed by Tokio.
    pub fn socket_address(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    /// Loads the bind host and port from a caller-provided variable reader for testability.
    fn from_reader(
        mut read_variable: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, WebBootstrapError> {
        let raw_host = read_variable(HOST_ENV_VAR).unwrap_or_else(|| DEFAULT_HOST.to_string());
        let host = raw_host
            .parse::<IpAddr>()
            .map_err(|source| WebBootstrapError::InvalidHost {
                value: raw_host.clone(),
                source,
            })?;
        let raw_port = read_variable(PORT_ENV_VAR).unwrap_or_else(|| DEFAULT_PORT.to_string());
        let port = raw_port
            .parse::<u16>()
            .map_err(|source| WebBootstrapError::InvalidPort {
                value: raw_port.clone(),
                source,
            })?;

        Ok(Self { host, port })
    }
}

/// Loads the logging configuration from the environment contract defined for the web server bootstrap.
fn read_logging_config(
    mut read_variable: impl FnMut(&str) -> Option<String>,
    timezone: chrono_tz::Tz,
) -> Result<LoggingConfig, WebBootstrapError> {
    let level = match read_variable(LOG_LEVEL_ENV_VAR)
        .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        value => {
            return Err(WebBootstrapError::InvalidLogLevel {
                value: value.to_string(),
            });
        }
    };
    let data_dir = read_data_dir_root(&mut read_variable)?;
    let file_config = FileLoggingConfig::new(
        data_dir.join("logs").join("ora.log"),
        RotationPolicy::Daily,
        read_log_max_days(&mut read_variable)?,
    );
    let output = match read_variable(LOG_MODE_ENV_VAR)
        .unwrap_or_else(|| DEFAULT_LOG_MODE.to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "stdout" => LogOutput::Stdout,
        "file" => LogOutput::File(file_config),
        "stdout_and_file" => LogOutput::StdoutAndFile(file_config),
        value => {
            return Err(WebBootstrapError::InvalidLogMode {
                value: value.to_string(),
            });
        }
    };

    Ok(LoggingConfig::new(level, output, timezone))
}

/// Parses the configured retention window and rejects zero-day values explicitly.
fn read_log_max_days(
    mut read_variable: impl FnMut(&str) -> Option<String>,
) -> Result<NonZeroUsize, WebBootstrapError> {
    let raw_value =
        read_variable(LOG_MAX_DAYS_ENV_VAR).unwrap_or_else(|| DEFAULT_LOG_MAX_DAYS.to_string());
    let parsed_value =
        raw_value
            .parse::<usize>()
            .map_err(|source| WebBootstrapError::InvalidLogMaxDays {
                value: raw_value.clone(),
                source,
            })?;

    NonZeroUsize::new(parsed_value).ok_or(WebBootstrapError::InvalidLogMaxDaysZero)
}

#[cfg(test)]
mod tests {
    use super::{
        DATA_DIR_ENV_VAR, DEFAULT_HOST, DEFAULT_PORT, DENO_PATH_ENV_VAR, DatabaseConfig,
        FileSystemConfig, HOME_ENV_VAR, HOST_ENV_VAR, LOG_MODE_ENV_VAR, PORT_ENV_VAR,
        RIPGREP_PATH_ENV_VAR, RuntimeBinaryPaths, RuntimeConfig, ServerConfig,
        WORKTREE_DIR_ENV_VAR, WorktreeConfig,
    };
    use crate::error::WebBootstrapError;
    use crate::timezone::{TimezoneSource, TimezoneWarning};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    /// Verifies the database configuration defaults to an absolute SQLite path under the current directory.
    #[test]
    fn loads_default_database_configuration() {
        let config = DatabaseConfig::from_reader(|_| None).unwrap_or_else(|error| {
            panic!("expected default database configuration to load: {error}");
        });
        let expected_path = std::env::current_dir().unwrap().join("ora.sqlite3");

        assert_eq!(config.path(), expected_path.as_path());
    }

    /// Verifies filesystem browsing starts from the absolute server user home directory.
    #[test]
    fn loads_file_system_home_directory() {
        let temp_dir = TempDir::new().unwrap();
        let config = FileSystemConfig::from_reader(|key| match key {
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected filesystem configuration to load: {error}"));

        assert_eq!(config.home_directory(), temp_dir.path());
    }

    /// Verifies the database configuration derives the SQLite path from `ORA_DATA_DIR`.
    #[test]
    fn loads_database_configuration_from_data_dir() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("state");
        let config = DatabaseConfig::from_reader(|key| match key {
            DATA_DIR_ENV_VAR => Some(data_dir.to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected data directory configuration to load: {error}"));

        let expected_path = data_dir.join("ora.sqlite3");

        assert_eq!(config.path(), expected_path.as_path());
    }

    /// Verifies empty data directories fail with a typed bootstrap error.
    #[test]
    fn rejects_empty_data_dir_configuration() {
        let error = match DatabaseConfig::from_reader(|key| match key {
            DATA_DIR_ENV_VAR => Some("   ".to_string()),
            _ => None,
        }) {
            Ok(_) => panic!("expected empty data directory configuration to fail"),
            Err(error) => error,
        };

        assert!(matches!(error, WebBootstrapError::InvalidDatabasePathEmpty));
    }

    /// Verifies the linked-worktree root defaults under the server user's home directory.
    #[test]
    fn loads_default_worktree_root_from_home_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_system = FileSystemConfig::from_reader(|key| match key {
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected filesystem configuration to load: {error}"));
        let config = WorktreeConfig::from_reader(|_| None, &file_system)
            .unwrap_or_else(|error| panic!("expected worktree configuration to load: {error}"));

        let expected_root = temp_dir.path().join(".ora").join("worktrees");

        assert_eq!(config.root(), expected_root.as_path());
    }

    /// Verifies `ORA_WORKTREE_DIR` overrides the home-derived linked-worktree root.
    #[test]
    fn loads_worktree_root_from_environment() {
        let temp_dir = TempDir::new().unwrap();
        let configured_root = temp_dir.path().join("isolated-worktrees");
        let file_system = FileSystemConfig::from_reader(|key| match key {
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected filesystem configuration to load: {error}"));
        let config = WorktreeConfig::from_reader(
            |key| match key {
                WORKTREE_DIR_ENV_VAR => Some(configured_root.to_string_lossy().to_string()),
                _ => None,
            },
            &file_system,
        )
        .unwrap_or_else(|error| panic!("expected worktree configuration to load: {error}"));

        assert_eq!(config.root(), configured_root.as_path());
    }

    /// Verifies an empty explicit linked-worktree root fails during configuration loading.
    #[test]
    fn rejects_empty_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let file_system = FileSystemConfig::from_reader(|key| match key {
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected filesystem configuration to load: {error}"));
        let error = match WorktreeConfig::from_reader(
            |key| match key {
                WORKTREE_DIR_ENV_VAR => Some("   ".to_string()),
                _ => None,
            },
            &file_system,
        ) {
            Ok(_) => panic!("expected empty worktree configuration to fail"),
            Err(error) => error,
        };

        assert!(matches!(error, WebBootstrapError::InvalidWorktreePathEmpty));
    }

    /// Verifies a relative explicit linked-worktree root cannot depend on process cwd.
    #[test]
    fn rejects_relative_worktree_root() {
        let temp_dir = TempDir::new().unwrap();
        let file_system = FileSystemConfig::from_reader(|key| match key {
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected filesystem configuration to load: {error}"));
        let error = match WorktreeConfig::from_reader(
            |key| match key {
                WORKTREE_DIR_ENV_VAR => Some("relative-worktrees".to_string()),
                _ => None,
            },
            &file_system,
        ) {
            Ok(_) => panic!("expected relative worktree configuration to fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            WebBootstrapError::WorktreeDirectoryNotAbsolute { worktree_directory }
                if worktree_directory == std::path::PathBuf::from("relative-worktrees")
        ));
    }

    /// Verifies the logging configuration derives the file path from `ORA_DATA_DIR`.
    #[test]
    fn loads_logging_configuration_from_data_dir() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("state");
        let config = super::read_logging_config(
            |key| match key {
                DATA_DIR_ENV_VAR => Some(data_dir.to_string_lossy().to_string()),
                LOG_MODE_ENV_VAR => Some("file".to_string()),
                _ => None,
            },
            chrono_tz::Asia::Shanghai,
        )
        .unwrap_or_else(|error| panic!("expected logging configuration to load: {error}"));

        match config.output {
            ora_logging::LogOutput::Stdout => {
                panic!("expected file-backed logging output");
            }
            ora_logging::LogOutput::File(file_config)
            | ora_logging::LogOutput::StdoutAndFile(file_config) => {
                let expected_path = data_dir.join("logs").join("ora.log");
                assert_eq!(file_config.path, expected_path);
            }
        }
    }

    /// Verifies the server configuration defaults to the documented host and port.
    #[test]
    fn loads_default_server_configuration() {
        let config = ServerConfig::from_reader(|_| None).unwrap_or_else(|error| {
            panic!("expected default server configuration to load: {error}");
        });

        assert_eq!(config.host().to_string(), DEFAULT_HOST.to_string());
        assert_eq!(config.port(), DEFAULT_PORT);
    }

    /// Verifies invalid port values fail with a typed bootstrap error.
    #[test]
    fn rejects_invalid_port_configuration() {
        let error = match ServerConfig::from_reader(|key| match key {
            HOST_ENV_VAR => Some(DEFAULT_HOST.to_string()),
            PORT_ENV_VAR => Some("not-a-port".to_string()),
            _ => None,
        }) {
            Ok(_) => panic!("expected invalid port configuration to fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            WebBootstrapError::InvalidPort { value, .. } if value == "not-a-port"
        ));
    }

    /// Verifies the runtime configuration loads both the server and logging contracts together.
    #[test]
    fn loads_runtime_configuration() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("state");
        let binary_path = std::env::current_exe().unwrap();
        let config = RuntimeConfig::from_reader(|key| match key {
            DATA_DIR_ENV_VAR => Some(data_dir.to_string_lossy().to_string()),
            LOG_MODE_ENV_VAR => Some("file".to_string()),
            HOME_ENV_VAR => Some(temp_dir.path().to_string_lossy().to_string()),
            RIPGREP_PATH_ENV_VAR | DENO_PATH_ENV_VAR => {
                Some(binary_path.to_string_lossy().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|error| panic!("expected runtime configuration to load: {error}"));

        let expected_database_path = data_dir.join("ora.sqlite3");
        let expected_worktree_root = temp_dir.path().join(".ora").join("worktrees");
        let expected_log_path = data_dir.join("logs").join("ora.log");

        assert_eq!(config.database().path(), expected_database_path.as_path());
        assert_eq!(config.worktree().root(), expected_worktree_root.as_path());
        assert_eq!(config.file_system().home_directory(), temp_dir.path());
        assert_eq!(
            config.binaries(),
            &RuntimeBinaryPaths::for_tests(&binary_path, &binary_path)
        );
        assert_eq!(config.logging().timezone, chrono_tz::Asia::Shanghai);
        assert_eq!(config.timezone_source(), TimezoneSource::Default);
        assert_eq!(
            config.timezone_warning(),
            Some(&TimezoneWarning::MissingConfiguration)
        );

        match &config.logging().output {
            ora_logging::LogOutput::Stdout => panic!("expected file-backed logging output"),
            ora_logging::LogOutput::File(file_config)
            | ora_logging::LogOutput::StdoutAndFile(file_config) => {
                assert_eq!(&file_config.path, &expected_log_path);
            }
        }
    }

    /// Verifies Web startup cannot silently fall back when one executable path is omitted.
    #[test]
    fn requires_both_binary_paths() {
        let binary_path = std::env::current_exe().unwrap();
        let error = RuntimeBinaryPaths::from_reader(|key| match key {
            RIPGREP_PATH_ENV_VAR => Some(binary_path.to_string_lossy().to_string()),
            _ => None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            WebBootstrapError::BinaryPathMissing {
                variable: DENO_PATH_ENV_VAR
            }
        ));
    }
}
