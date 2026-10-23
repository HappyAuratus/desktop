//! Read-only collection of the reserved artifact directory.
//!
//! The rules are the ones local scheduled runs apply — the same fixed directory, the same closed
//! type whitelist, the same path, link, size, count and total bounds — but the result is a
//! declaration for Cloud instead of a copy into local storage: Cloud generates the object keys and
//! verifies the uploads, so nothing here writes into the Workspace or decides where a file lives.
//!
//! A missing directory is "no artifacts"; every refused candidate is recorded with its reason so a
//! refusal is visible to the reader instead of silently dropped.

use ora_domain::{
    MAX_SCHEDULED_ARTIFACT_BYTES, MAX_SCHEDULED_ARTIFACT_TOTAL_BYTES,
    MAX_SCHEDULED_ARTIFACTS_PER_RUN, SCHEDULED_ARTIFACT_DIRECTORY, ScheduledArtifactKind,
};
use ora_node_protocol::{
    ScheduledArtifactItem, ScheduledArtifactManifest, ScheduledArtifactRejection,
    ScheduledArtifactRejectionReason,
};
use ora_utils::path::{StrictRelativePath, has_multiple_hard_links};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Collects what one run declared, or `None` when it has no artifact directory.
pub(super) fn collect(checkout: &Path) -> Option<ScheduledArtifactManifest> {
    let directory = checkout.join(SCHEDULED_ARTIFACT_DIRECTORY);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(_) => return Some(refused(ScheduledArtifactRejectionReason::ReadFailed)),
    };
    // The directory is Ora's own convention, so a link or a regular file under that name is not a
    // declaration to follow: following it could collect files from outside the Workspace.
    if metadata.file_type().is_symlink() {
        return Some(refused(ScheduledArtifactRejectionReason::Symlink));
    }
    if !metadata.is_dir() {
        return Some(refused(ScheduledArtifactRejectionReason::PathUnsafe));
    }
    let mut collector = Collector::default();
    collector.visit(&directory, "");
    Some(ScheduledArtifactManifest {
        items: collector.items,
        rejections: collector.rejections,
    })
}

/// Refuses the whole directory, which has no candidate path of its own.
fn refused(reason: ScheduledArtifactRejectionReason) -> ScheduledArtifactManifest {
    ScheduledArtifactManifest {
        items: Vec::new(),
        rejections: vec![ScheduledArtifactRejection {
            display_name: SCHEDULED_ARTIFACT_DIRECTORY.to_string(),
            reason,
        }],
    }
}

/// One directory walk that carries the accepted items, refusals and the bounds already consumed.
#[derive(Default)]
struct Collector {
    items: Vec<ScheduledArtifactItem>,
    rejections: Vec<ScheduledArtifactRejection>,
    /// Folded-case keys of every accepted and refused name, so one path cannot be declared twice
    /// under two spellings that a case-insensitive or non-normalized reader would confuse.
    names: HashSet<String>,
    total: u64,
}

impl Collector {
    /// Walks one directory level by level without following links, collecting accepted files.
    fn visit(&mut self, directory: &Path, prefix: &str) {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                self.refuse(
                    directory_name(prefix),
                    ScheduledArtifactRejectionReason::ReadFailed,
                );
                return;
            }
        };
        let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(_) => {
                self.refuse(
                    directory_name(prefix),
                    ScheduledArtifactRejectionReason::ReadFailed,
                );
                return;
            }
        };
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                self.refuse(
                    file_name.to_string_lossy().into_owned(),
                    ScheduledArtifactRejectionReason::PathUnsafe,
                );
                continue;
            };
            let relative = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            let Some(parsed) = declared_path(&relative) else {
                self.refuse(relative, ScheduledArtifactRejectionReason::PathUnsafe);
                continue;
            };
            if !self.names.insert(parsed.fold_case_key()) {
                self.refuse(relative, ScheduledArtifactRejectionReason::PathUnsafe);
                continue;
            }
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.refuse(relative, ScheduledArtifactRejectionReason::ReadFailed);
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                self.refuse(relative, ScheduledArtifactRejectionReason::Symlink);
                continue;
            }
            if metadata.is_dir() {
                self.visit(&entry.path(), parsed.as_str());
                continue;
            }
            if !metadata.is_file() {
                self.refuse(relative, ScheduledArtifactRejectionReason::PathUnsafe);
                continue;
            }
            self.accept(&entry.path(), &parsed, &metadata);
        }
    }

    /// Applies the type, link, count, size and total rules before a file becomes a declared item.
    fn accept(&mut self, source: &Path, relative: &StrictRelativePath, metadata: &fs::Metadata) {
        let name = relative.as_str().to_string();
        let Some(kind) = kind_of(relative.file_name().unwrap_or_default()) else {
            self.refuse(name, ScheduledArtifactRejectionReason::UnsupportedType);
            return;
        };
        match has_multiple_hard_links(source) {
            Ok(false) => {}
            Ok(true) => {
                self.refuse(name, ScheduledArtifactRejectionReason::HardLink);
                return;
            }
            Err(_) => {
                self.refuse(name, ScheduledArtifactRejectionReason::ReadFailed);
                return;
            }
        }
        if metadata.len() > MAX_SCHEDULED_ARTIFACT_BYTES {
            self.refuse(name, ScheduledArtifactRejectionReason::TooLarge);
            return;
        }
        if self.items.len() >= MAX_SCHEDULED_ARTIFACTS_PER_RUN {
            self.refuse(name, ScheduledArtifactRejectionReason::TooMany);
            return;
        }
        if self.total.saturating_add(metadata.len()) > MAX_SCHEDULED_ARTIFACT_TOTAL_BYTES {
            self.refuse(name, ScheduledArtifactRejectionReason::TotalTooLarge);
            return;
        }
        let (bytes, sha256) = match read_bounded(source) {
            Ok(read) => read,
            Err(_) => {
                self.refuse(name, ScheduledArtifactRejectionReason::ReadFailed);
                return;
            }
        };
        // A file that shrank, grew or changed between its metadata probe and the read cannot be
        // uploaded as the digest Cloud verifies, so it is refused rather than misdeclared.
        if bytes.len() as u64 > MAX_SCHEDULED_ARTIFACT_BYTES {
            self.refuse(name, ScheduledArtifactRejectionReason::TooLarge);
            return;
        }
        if bytes.len() as u64 != metadata.len() {
            self.refuse(name, ScheduledArtifactRejectionReason::ReadFailed);
            return;
        }
        self.total = self.total.saturating_add(bytes.len() as u64);
        self.items.push(ScheduledArtifactItem {
            index: self.items.len() as u32,
            display_name: name,
            kind,
            size_bytes: bytes.len() as u64,
            sha256,
        });
    }

    /// Records one refusal; the reader sees the reason instead of a silently missing artifact.
    fn refuse(&mut self, display_name: String, reason: ScheduledArtifactRejectionReason) {
        self.rejections.push(ScheduledArtifactRejection {
            display_name,
            reason,
        });
    }
}

/// Names the refused directory: the root has only its fixed name, nested levels their relative path.
fn directory_name(prefix: &str) -> String {
    if prefix.is_empty() {
        SCHEDULED_ARTIFACT_DIRECTORY.to_string()
    } else {
        prefix.to_string()
    }
}

/// Parses one declared path, refusing a Windows alternate data stream `:` that is legal on Unix.
fn declared_path(raw: &str) -> Option<StrictRelativePath> {
    if raw.contains(':') {
        return None;
    }
    StrictRelativePath::parse(raw).ok()
}

/// Derives the declared kind from an extension; anything outside the whitelist is refused.
fn kind_of(file_name: &str) -> Option<ora_node_protocol::ScheduledArtifactKind> {
    match ScheduledArtifactKind::from_file_name(file_name)? {
        ScheduledArtifactKind::Markdown => Some(ora_node_protocol::ScheduledArtifactKind::Markdown),
        ScheduledArtifactKind::Json => Some(ora_node_protocol::ScheduledArtifactKind::Json),
        ScheduledArtifactKind::Text => Some(ora_node_protocol::ScheduledArtifactKind::Text),
        ScheduledArtifactKind::Log => Some(ora_node_protocol::ScheduledArtifactKind::Log),
        ScheduledArtifactKind::Xml => Some(ora_node_protocol::ScheduledArtifactKind::Xml),
        ScheduledArtifactKind::Html => Some(ora_node_protocol::ScheduledArtifactKind::Html),
    }
}

/// Reads at most one byte past the single-artifact bound, returning the bytes and their digest.
///
/// Bounding the read keeps a file that grows after its metadata probe from filling memory, and the
/// caller compares the length it got with the length it was promised.
fn read_bounded(source: &Path) -> io::Result<(Vec<u8>, String)> {
    let mut file = fs::File::open(source)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SCHEDULED_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let sha256 = ora_utils::hash::sha256_hex(&bytes);
    Ok((bytes, sha256))
}
