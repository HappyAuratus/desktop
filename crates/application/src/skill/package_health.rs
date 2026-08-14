use super::storage::{CreateHandle, SkillStorage, SwapHandle};
use crate::ApplicationError;
use ora_contracts::SkillAvailability;
use ora_skill_package::Limits;
use ora_skill_package::manifest::parse_manifest;
use std::path::Path;

/// Handle for a newly promoted formal package that may have replaced an untracked leftover.
pub(crate) enum PromotedPackage {
    Created(CreateHandle),
    Replaced(SwapHandle),
}

impl PromotedPackage {
    /// Restores the previous formal directory when the database write did not commit.
    pub(crate) fn rollback<Storage: SkillStorage>(&self, storage: &Storage) {
        match self {
            Self::Created(handle) => {
                let _ = storage.rollback_create(handle);
            }
            Self::Replaced(handle) => {
                let _ = storage.rollback_swap(handle);
            }
        }
    }

    /// Removes journal and compensation artifacts after the database write succeeds.
    pub(crate) fn finish<Storage: SkillStorage>(
        &self,
        storage: &Storage,
    ) -> Result<(), ApplicationError> {
        match self {
            Self::Created(handle) => storage.finish_create(handle),
            Self::Replaced(handle) => storage.finish_swap(handle),
        }
        .map_err(ApplicationError::from_skill_storage_error)
    }
}

/// Returns whether `SKILL.md` bytes parse as a skill manifest.
pub(crate) fn manifest_is_usable(bytes: &[u8]) -> bool {
    parse_manifest(bytes, Limits::default().max_manifest_bytes).is_ok()
}

/// Returns whether the formal directory has a root `SKILL.md` that parses as a skill manifest.
pub fn has_usable_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<bool, ApplicationError> {
    Ok(storage
        .read_manifest(name)
        .map_err(ApplicationError::from_skill_storage_error)?
        .is_some_and(|bytes| manifest_is_usable(&bytes)))
}

/// Derives catalog availability from the on-disk package. Missing or unreadable files do
/// not forget the row; the skill stays visible as unavailable until the user deletes it
/// or restores a package with the same name.
pub(crate) fn package_availability<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<SkillAvailability, ApplicationError> {
    if has_usable_package(storage, name)? {
        Ok(SkillAvailability::Available)
    } else {
        Ok(SkillAvailability::Unavailable)
    }
}

/// Removes a leftover directory only when it has no root `SKILL.md`.
///
/// A present `SKILL.md` stays on disk even when it cannot be parsed, matching startup
/// reconciliation. Unclaimed create/import replace leftovers through
/// [`commit_unclaimed_package`]; restore uses [`commit_restored_package`] so a usable
/// package is not overwritten.
pub(crate) fn clear_incomplete_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<(), ApplicationError> {
    if !storage.formal_exists(name)
        || storage
            .read_manifest(name)
            .map_err(ApplicationError::from_skill_storage_error)?
            .is_some()
    {
        return Ok(());
    }
    storage
        .remove_formal(name)
        .map_err(ApplicationError::from_skill_storage_error)
}

/// Frees a catalog name after a successful delete when `commit_delete` already saw no directory.
///
/// Create and import of an unclaimed name must call [`commit_unclaimed_package`] instead so a
/// leftover complete package is journaled and can be restored.
pub(crate) fn claim_untracked_name<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<(), ApplicationError> {
    storage
        .remove_formal(name)
        .map_err(ApplicationError::from_skill_storage_error)
}

/// Promotes staging into `<name>`, replacing an existing leftover through a journaled swap.
///
/// `commit_create` cannot succeed while the formal directory exists, so claiming an untracked
/// name must not delete first. A swap keeps the leftover in the compensation backup until the
/// database write commits.
pub(crate) fn commit_unclaimed_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
    staging: &Path,
) -> Result<PromotedPackage, ApplicationError> {
    promote_staging(storage, name, staging)
}

/// Promotes staging onto an unavailable catalog name without overwriting a usable package.
///
/// Restore must not use [`commit_unclaimed_package`]: that helper claims even a complete leftover.
/// If another request restored the package first, this returns [`ApplicationError::SkillNameConflict`].
/// A missing directory still uses `commit_create`, which fails if a directory appears in between.
pub(crate) fn commit_restored_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
    staging: &Path,
) -> Result<PromotedPackage, ApplicationError> {
    if has_usable_package(storage, name)? {
        return Err(ApplicationError::SkillNameConflict {
            name: name.to_string(),
        });
    }
    promote_staging(storage, name, staging)
}

/// Promotes staging into `<name>`, swapping when a leftover directory is already present.
fn promote_staging<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
    staging: &Path,
) -> Result<PromotedPackage, ApplicationError> {
    if storage.formal_exists(name) {
        storage
            .commit_swap(name, name, staging)
            .map(PromotedPackage::Replaced)
            .map_err(ApplicationError::from_skill_storage_error)
    } else {
        storage
            .commit_create(name, staging)
            .map(PromotedPackage::Created)
            .map_err(ApplicationError::from_skill_storage_error)
    }
}
