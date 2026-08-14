use crate::skill::mapper::{map_skill, map_skill_details};
use crate::skill::package_health::{
    claim_untracked_name, clear_incomplete_package, has_usable_package, package_availability,
};
use crate::skill::ports::{SkillIdGenerator, SkillRepository};
use crate::skill::storage::{SkillStorage, SkillStorageError};
use crate::{ApplicationError, Clock};
use gray_matter::{Matter, ParsedEntity, engine::YAML};
use ora_contracts::{
    CreateSkillRequest, CreateSkillResponse, DeleteSkillRequest, DeleteSkillResponse,
    GetSkillRequest, GetSkillResponse, ListSkillsRequest, ListSkillsResponse, SkillAvailability,
    UpdateSkillRequest, UpdateSkillResponse,
};
use ora_domain::{AuditFields, Namespace, Skill, SkillId};
use ora_skill_package::manifest::{render_manifest, rewrite_manifest, rewrite_manifest_body};
use serde_json::Value;

/// Handles atomic creation of a reusable skill definition (database plus formal directory).
pub struct CreateSkillHandler<Repository, Storage, IdGenerator, ClockSource> {
    repository: Repository,
    storage: Storage,
    id_generator: IdGenerator,
    clock: ClockSource,
}

impl<Repository, Storage, IdGenerator, ClockSource>
    CreateSkillHandler<Repository, Storage, IdGenerator, ClockSource>
{
    pub fn new(
        repository: Repository,
        storage: Storage,
        id_generator: IdGenerator,
        clock: ClockSource,
    ) -> Self {
        Self {
            repository,
            storage,
            id_generator,
            clock,
        }
    }
}

impl<Repository, Storage, IdGenerator, ClockSource>
    CreateSkillHandler<Repository, Storage, IdGenerator, ClockSource>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
    IdGenerator: SkillIdGenerator,
    ClockSource: Clock,
{
    /// Creates a normalized skill and its minimal manifest atomically.
    pub fn handle(
        &self,
        request: CreateSkillRequest,
    ) -> Result<CreateSkillResponse, ApplicationError> {
        let name = request.name.trim().to_string();
        let namespace = Namespace::local();
        let now = self.clock.now_timestamp_millis();
        if let Some(existing) = self
            .repository
            .find_skill_by_name(&namespace, &name)
            .map_err(ApplicationError::from_skill_repository_error)?
        {
            if has_usable_package(&self.storage, &existing.name)? {
                return Err(ApplicationError::SkillNameConflict {
                    namespace: namespace.to_string(),
                    name,
                });
            }
            let restored = restore_unavailable_skill(
                &self.repository,
                &self.storage,
                existing,
                name,
                request.description,
                request.content.as_deref(),
                now,
            )?;
            return Ok(CreateSkillResponse {
                skill: map_skill(restored, SkillAvailability::Available),
            });
        }

        let skill = Skill::new(
            self.id_generator.generate_skill_id(),
            namespace,
            name,
            request.description,
            AuditFields::new(now, now, false),
        )
        .map_err(ApplicationError::from_skill_domain_error)?;
        claim_untracked_name(&self.storage, &skill.name)?;

        let staging = self
            .storage
            .create_staging()
            .map_err(ApplicationError::from_skill_storage_error)?;
        let manifest = render_manifest(
            &skill.name,
            &skill.description,
            request.content.as_deref().unwrap_or(""),
        );
        self.storage
            .write_manifest(&staging, manifest.as_bytes())
            .map_err(ApplicationError::from_skill_storage_error)?;
        let handle = self
            .storage
            .commit_create(&skill.name, &staging)
            .map_err(ApplicationError::from_skill_storage_error)?;
        let created = self.repository.create_skill(skill).map_err(|error| {
            let _ = self.storage.rollback_create(&handle);
            ApplicationError::from_skill_repository_error(error)
        })?;
        self.storage
            .finish_create(&handle)
            .map_err(ApplicationError::from_skill_storage_error)?;

        Ok(CreateSkillResponse {
            skill: map_skill(created, SkillAvailability::Available),
        })
    }
}

/// Handles lookup of one reusable skill definition.
pub struct GetSkillHandler<Repository, Storage> {
    repository: Repository,
    storage: Storage,
}

impl<Repository, Storage> GetSkillHandler<Repository, Storage> {
    pub fn new(repository: Repository, storage: Storage) -> Self {
        Self {
            repository,
            storage,
        }
    }
}

impl<Repository, Storage> GetSkillHandler<Repository, Storage>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
{
    /// Loads one visible skill or reports a stable not-found error.
    pub fn handle(&self, request: GetSkillRequest) -> Result<GetSkillResponse, ApplicationError> {
        let skill_id = SkillId::new(request.skill_id);
        let skill = self
            .repository
            .find_skill(&skill_id)
            .map_err(ApplicationError::from_skill_repository_error)?
            .ok_or_else(|| ApplicationError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })?;
        let manifest = self
            .storage
            .read_manifest(&skill.name)
            .map_err(ApplicationError::from_skill_storage_error)?;
        let Some(manifest) = manifest else {
            return Ok(GetSkillResponse {
                skill: map_skill_details(skill, String::new(), SkillAvailability::Unavailable),
            });
        };
        let text = String::from_utf8(manifest).map_err(|_| {
            ApplicationError::from_manifest_error(ora_skill_package::ManifestError::YamlInvalid)
        })?;
        let parsed: ParsedEntity<Value> = Matter::<YAML>::new().parse(&text).map_err(|_| {
            ApplicationError::from_manifest_error(ora_skill_package::ManifestError::YamlInvalid)
        })?;
        Ok(GetSkillResponse {
            skill: map_skill_details(skill, parsed.content, SkillAvailability::Available),
        })
    }
}

/// Handles listing reusable skill definitions.
pub struct ListSkillsHandler<Repository, Storage> {
    repository: Repository,
    storage: Storage,
}

impl<Repository, Storage> ListSkillsHandler<Repository, Storage> {
    pub fn new(repository: Repository, storage: Storage) -> Self {
        Self {
            repository,
            storage,
        }
    }
}

impl<Repository, Storage> ListSkillsHandler<Repository, Storage>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
{
    /// Lists every visible skill and reports whether its formal package is still present.
    pub fn handle(
        &self,
        _request: ListSkillsRequest,
    ) -> Result<ListSkillsResponse, ApplicationError> {
        let skills = self
            .repository
            .list_skills()
            .map_err(ApplicationError::from_skill_repository_error)?;
        let mut mapped = Vec::new();
        for skill in skills {
            let availability = package_availability(&self.storage, &skill.name)?;
            mapped.push(map_skill(skill, availability));
        }
        Ok(ListSkillsResponse { skills: mapped })
    }
}

/// Handles atomic replacement of reusable skill definitions including folder renames.
pub struct UpdateSkillHandler<Repository, Storage, ClockSource> {
    repository: Repository,
    storage: Storage,
    clock: ClockSource,
}

impl<Repository, Storage, ClockSource> UpdateSkillHandler<Repository, Storage, ClockSource> {
    pub fn new(repository: Repository, storage: Storage, clock: ClockSource) -> Self {
        Self {
            repository,
            storage,
            clock,
        }
    }
}

impl<Repository, Storage, ClockSource> UpdateSkillHandler<Repository, Storage, ClockSource>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
    ClockSource: Clock,
{
    /// Replaces editable skill fields while preserving identity, creation time, and package
    /// content that the user did not modify.
    pub fn handle(
        &self,
        request: UpdateSkillRequest,
    ) -> Result<UpdateSkillResponse, ApplicationError> {
        let skill_id = SkillId::new(request.skill_id);
        let existing = self
            .repository
            .find_skill(&skill_id)
            .map_err(ApplicationError::from_skill_repository_error)?
            .ok_or_else(|| ApplicationError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })?;

        let name = request.name.trim().to_string();
        reject_conflicting_name(&self.repository, &existing.namespace, &name, &existing.id)?;
        if !has_usable_package(&self.storage, &existing.name)? {
            let restored = restore_unavailable_skill(
                &self.repository,
                &self.storage,
                existing,
                name,
                request.description,
                request.content.as_deref(),
                self.clock.now_timestamp_millis(),
            )?;
            return Ok(UpdateSkillResponse {
                skill: map_skill(restored, SkillAvailability::Available),
            });
        }

        let skill = Skill::new(
            skill_id,
            existing.namespace,
            name,
            request.description,
            AuditFields::new(
                existing.audit_fields.created_at,
                self.clock.now_timestamp_millis(),
                false,
            ),
        )
        .map_err(ApplicationError::from_skill_domain_error)?;

        let staging = self
            .storage
            .create_staging()
            .map_err(ApplicationError::from_skill_storage_error)?;
        // Preserve every package file the user did not modify; only the manifest is rewritten.
        self.storage
            .stage_existing(&existing.name, &staging)
            .map_err(ApplicationError::from_skill_storage_error)?;
        let rewritten = match self
            .storage
            .read_manifest(&existing.name)
            .map_err(ApplicationError::from_skill_storage_error)?
        {
            Some(content) => match request.content.as_deref() {
                Some(body) => {
                    rewrite_manifest_body(&content, &skill.name, &skill.description, body)
                }
                None => rewrite_manifest(&content, &skill.name, &skill.description),
            }
            .map_err(ApplicationError::from_manifest_error)?,
            None => render_manifest(
                &skill.name,
                &skill.description,
                request.content.as_deref().unwrap_or(""),
            ),
        };
        self.storage
            .write_manifest(&staging, rewritten.as_bytes())
            .map_err(ApplicationError::from_skill_storage_error)?;
        let handle = self
            .storage
            .commit_swap(&skill.name, &existing.name, &staging)
            .map_err(ApplicationError::from_skill_storage_error)?;
        let updated = self.repository.update_skill(skill).map_err(|error| {
            let _ = self.storage.rollback_swap(&handle);
            ApplicationError::from_skill_repository_error(error)
        })?;
        self.storage
            .finish_swap(&handle)
            .map_err(ApplicationError::from_skill_storage_error)?;

        Ok(UpdateSkillResponse {
            skill: map_skill(updated, SkillAvailability::Available),
        })
    }
}

/// Handles atomic soft deletion of reusable skill definitions and their formal directories.
pub struct DeleteSkillHandler<Repository, Storage, ClockSource> {
    repository: Repository,
    storage: Storage,
    clock: ClockSource,
}

impl<Repository, Storage, ClockSource> DeleteSkillHandler<Repository, Storage, ClockSource> {
    pub fn new(repository: Repository, storage: Storage, clock: ClockSource) -> Self {
        Self {
            repository,
            storage,
            clock,
        }
    }
}

impl<Repository, Storage, ClockSource> DeleteSkillHandler<Repository, Storage, ClockSource>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
    ClockSource: Clock,
{
    /// Soft-deletes one visible skill and removes its formal directory atomically.
    pub fn handle(
        &self,
        request: DeleteSkillRequest,
    ) -> Result<DeleteSkillResponse, ApplicationError> {
        let skill_id = SkillId::new(request.skill_id);
        let existing = self
            .repository
            .find_skill(&skill_id)
            .map_err(ApplicationError::from_skill_repository_error)?
            .ok_or_else(|| ApplicationError::SkillNotFound {
                skill_id: skill_id.to_string(),
            })?;

        let handle = match self.storage.commit_delete(&existing.name) {
            Ok(handle) => Some(handle),
            Err(SkillStorageError::FormalDirectoryMissing { .. }) => None,
            Err(error) => return Err(ApplicationError::from_skill_storage_error(error)),
        };
        let deleted = self
            .repository
            .soft_delete_skill(&skill_id, self.clock.now_timestamp_millis())
            .map_err(|error| {
                if let Some(handle) = &handle {
                    let _ = self.storage.rollback_delete(handle);
                }
                ApplicationError::from_skill_repository_error(error)
            })?;
        if !deleted {
            if let Some(handle) = &handle {
                let _ = self.storage.rollback_delete(handle);
            }
            return Err(ApplicationError::SkillNotFound {
                skill_id: skill_id.to_string(),
            });
        }
        if let Some(handle) = handle {
            self.storage
                .finish_delete(&handle)
                .map_err(ApplicationError::from_skill_storage_error)?;
        } else {
            // The catalog row is gone; free the name so a later import is not blocked
            // by a leftover that `exists()` missed when the package was already absent.
            claim_untracked_name(&self.storage, &existing.name)?;
        }

        Ok(DeleteSkillResponse {
            skill_id: skill_id.to_string(),
        })
    }
}

/// Rejects a rename that would collide with a different visible skill.
fn reject_conflicting_name<Repository: SkillRepository>(
    repository: &Repository,
    namespace: &Namespace,
    name: &str,
    own_id: &SkillId,
) -> Result<(), ApplicationError> {
    match repository
        .find_skill_by_name(namespace, name)
        .map_err(ApplicationError::from_skill_repository_error)?
    {
        Some(other) if &other.id != own_id => Err(ApplicationError::SkillNameConflict {
            namespace: namespace.to_string(),
            name: name.to_string(),
        }),
        _ => Ok(()),
    }
}

/// Writes a new formal package onto an unavailable catalog row, preserving its identity.
fn restore_unavailable_skill<Repository, Storage>(
    repository: &Repository,
    storage: &Storage,
    existing: Skill,
    name: String,
    description: String,
    content: Option<&str>,
    now: i64,
) -> Result<Skill, ApplicationError>
where
    Repository: SkillRepository,
    Storage: SkillStorage,
{
    let skill = Skill::new(
        existing.id.clone(),
        existing.namespace.clone(),
        name,
        description,
        AuditFields::new(existing.audit_fields.created_at, now, false),
    )
    .map_err(ApplicationError::from_skill_domain_error)?;
    clear_incomplete_package(storage, &existing.name)?;
    if skill.name != existing.name {
        if has_usable_package(storage, &skill.name)? {
            return Err(ApplicationError::SkillNameConflict {
                namespace: skill.namespace.to_string(),
                name: skill.name,
            });
        }
        clear_incomplete_package(storage, &skill.name)?;
    }
    let staging = storage
        .create_staging()
        .map_err(ApplicationError::from_skill_storage_error)?;
    let manifest = render_manifest(&skill.name, &skill.description, content.unwrap_or(""));
    storage
        .write_manifest(&staging, manifest.as_bytes())
        .map_err(ApplicationError::from_skill_storage_error)?;
    let handle = storage
        .commit_create(&skill.name, &staging)
        .map_err(ApplicationError::from_skill_storage_error)?;
    let updated = repository.update_skill(skill).map_err(|error| {
        let _ = storage.rollback_create(&handle);
        ApplicationError::from_skill_repository_error(error)
    })?;
    storage
        .finish_create(&handle)
        .map_err(ApplicationError::from_skill_storage_error)?;
    Ok(updated)
}
