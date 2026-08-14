use super::storage::SkillStorage;
use crate::ApplicationError;
use ora_contracts::SkillAvailability;

/// Returns whether the formal directory exists and contains a root `SKILL.md`.
pub(crate) fn has_usable_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<bool, ApplicationError> {
    if !storage.formal_exists(name) {
        return Ok(false);
    }
    Ok(storage
        .read_manifest(name)
        .map_err(ApplicationError::from_skill_storage_error)?
        .is_some())
}

/// Derives catalog availability from the on-disk package. Missing files do not
/// forget the row; the skill stays visible as unavailable until the user deletes
/// it or restores a package with the same name.
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

/// Removes a leftover directory only when it is not a usable skill package.
///
/// Complete untracked packages are left in place, matching startup reconciliation.
/// Create and import of an unclaimed name should call [`claim_untracked_name`] instead.
pub(crate) fn clear_incomplete_package<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<(), ApplicationError> {
    if !storage.formal_exists(name) || has_usable_package(storage, name)? {
        return Ok(());
    }
    storage
        .remove_formal(name)
        .map_err(ApplicationError::from_skill_storage_error)
}

/// Frees a catalog name that no visible row owns, including leftover complete packages.
///
/// Startup leaves untracked packages in place so a rename outside the app is not destroyed.
/// Once the user creates or imports that name, the leftover would block `commit_create` and
/// make a deleted skill look un-installable.
pub(crate) fn claim_untracked_name<Storage: SkillStorage>(
    storage: &Storage,
    name: &str,
) -> Result<(), ApplicationError> {
    storage
        .remove_formal(name)
        .map_err(ApplicationError::from_skill_storage_error)
}
