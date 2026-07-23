use std::sync::OnceLock;

use chrono::{Offset as _, TimeZone as _};
use time::{OffsetDateTime, UtcOffset};

const APP_TIMEZONE_VAR: &str = "DASHBOARD_TIMEZONE";
const SYSTEM_TIMEZONE_VAR: &str = "TZ";
const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";

/// Caches the process-wide timezone so the environment is read and parsed at most once.
///
/// The first caller resolves the timezone from the environment; every later caller reuses the
/// cached `chrono_tz::Tz`. This keeps hot paths like `now_local()` allocation-free after warmup
/// and pins the timezone for the process lifetime, so a mid-run environment change cannot shift
/// wall-clock offsets underneath already-recorded data.
static CONFIGURED_TIMEZONE: OnceLock<chrono_tz::Tz> = OnceLock::new();

/// Returns the current local time using chrono-tz instead of the platform localtime APIs.
pub fn now_local() -> OffsetDateTime {
    let utc_now = chrono::Utc::now();
    let offset = offset_for_unix_timestamp(utc_now.timestamp());
    let nanos = i128::from(utc_now.timestamp()) * 1_000_000_000
        + i128::from(utc_now.timestamp_subsec_nanos());

    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .to_offset(offset)
}

/// Returns the current Unix timestamp in milliseconds using the configured local timezone.
///
/// The nanosecond value is narrowed to i64 only *after* dividing, so the division runs on the full
/// i128 range and the cast can never truncate meaningful magnitude.
pub fn now_millis() -> i64 {
    (now_local().unix_timestamp_nanos() / 1_000_000) as i64
}

/// Returns the current configured local UTC offset.
pub fn local_offset() -> UtcOffset {
    offset_for_unix_timestamp(chrono::Utc::now().timestamp())
}

/// Returns the configured local UTC offset in milliseconds for SQL wall-clock reconstruction.
pub fn local_offset_millis() -> i64 {
    i64::from(local_offset().whole_seconds()) * 1000
}

/// Chooses the configured IANA timezone name without requiring global unsafe time crate setup.
fn configured_timezone_name() -> String {
    [APP_TIMEZONE_VAR, SYSTEM_TIMEZONE_VAR]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_TIMEZONE.to_string())
}

/// Returns the cached process timezone, resolving and validating it from the environment on first use.
///
/// An unparseable deployment configuration is reported once via `ora_warn!` and falls back to UTC,
/// so misconfiguration stays visible instead of being swallowed silently.
fn configured_timezone() -> chrono_tz::Tz {
    *CONFIGURED_TIMEZONE.get_or_init(|| {
        let name = configured_timezone_name();
        match name.parse::<chrono_tz::Tz>() {
            Ok(timezone) => timezone,
            Err(_) => {
                crate::ora_warn!(
                    message = "invalid timezone configuration, falling back to UTC",
                    timezone = %name,
                );
                chrono_tz::UTC
            }
        }
    })
}

/// Resolves the offset for a specific instant, so DST transitions are handled by chrono-tz data.
fn offset_for_unix_timestamp(timestamp: i64) -> UtcOffset {
    let local = configured_timezone().timestamp_opt(timestamp, 0);
    let offset_seconds = local
        .single()
        .map(|dt| dt.offset().fix().local_minus_utc())
        .unwrap_or(0);

    offset_from_seconds(offset_seconds)
}

/// Converts chrono's offset seconds into time's offset type while keeping invalid data contained.
fn offset_from_seconds(offset_seconds: i32) -> UtcOffset {
    UtcOffset::from_whole_seconds(offset_seconds).unwrap_or(UtcOffset::UTC)
}
