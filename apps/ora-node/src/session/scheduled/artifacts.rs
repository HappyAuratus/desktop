//! Read-only collection of the reserved artifact directory.
//!
//! The rules are the ones local scheduled runs apply — the same fixed directory, the same closed
//! type whitelist, the same path, link, size, count and total bounds — but the result is a
//! declaration for Cloud instead of a copy into local storage: Cloud generates the object keys and
//! verifies the uploads, so nothing here writes into the Workspace or decides where a file lives.
//!
//! A missing directory is "no artifacts"; every refused candidate is recorded with its reason so a
//! refusal is visible to the reader instead of silently dropped. The walk itself is bounded, so a
//! directory an agent flooded costs no more than the walk may examine and states when it was cut.

use ora_domain::{
    MAX_SCHEDULED_ARTIFACT_BYTES, MAX_SCHEDULED_ARTIFACT_TOTAL_BYTES,
    MAX_SCHEDULED_ARTIFACTS_PER_RUN, SCHEDULED_ARTIFACT_DIRECTORY, ScheduledArtifactKind,
};
use ora_node_protocol::{
    MAX_SCHEDULED_ARTIFACT_REJECTION_NAME_BYTES, MAX_SCHEDULED_ARTIFACT_REJECTIONS,
    ScheduledArtifactItem, ScheduledArtifactManifest, ScheduledArtifactRejection,
    ScheduledArtifactRejectionReason,
};
use ora_utils::path::{StrictRelativePath, has_multiple_hard_links};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Most directory entries one walk examines.
///
/// The per-run artifact bound is 64, so a legitimate run needs far fewer candidates than this. The
/// bound exists for the accidental case — a directory an agent filled with thousands of names —
/// where an unbounded walk would keep the session from settling. It is spent while a directory is
/// read, so neither memory nor enumeration grows past it, and the manifest states that the walk was
/// cut instead of letting a full refusal list stand for the whole directory.
const MAX_WALKED_CANDIDATES: usize = 512;

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
    let mut collector = Collector::new();
    collector.visit(&directory, "");
    Some(ScheduledArtifactManifest {
        items: collector.items,
        rejections: collector.rejections,
        omitted_rejections: collector.omitted_rejections,
        walk_truncated: collector.walk_truncated,
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
        omitted_rejections: 0,
        walk_truncated: false,
    }
}

/// The entries one directory read hands to the walk, and whether more remained.
pub(super) struct BoundedEntries<T> {
    pub(super) entries: Vec<T>,
    /// Whether the directory held entries the walk did not read, so nothing it read was dropped.
    pub(super) truncated: bool,
}

/// Reads at most `budget` entries of one directory, so the walk never holds or sorts a directory
/// larger than what it may still examine.
///
/// One extra probe separates a directory that ended exactly at the bound from one the walk cut
/// short, which is the difference between a complete declaration and a truncated one.
pub(super) fn bounded_entries<T>(
    mut entries: impl Iterator<Item = T>,
    budget: usize,
) -> BoundedEntries<T> {
    let mut kept = Vec::with_capacity(budget);
    while kept.len() < budget {
        match entries.next() {
            Some(entry) => kept.push(entry),
            None => {
                return BoundedEntries {
                    entries: kept,
                    truncated: false,
                };
            }
        }
    }
    BoundedEntries {
        entries: kept,
        truncated: entries.next().is_some(),
    }
}

/// One directory walk that carries the accepted items, the refusals still listable, and the bounds
/// already consumed.
struct Collector {
    items: Vec<ScheduledArtifactItem>,
    rejections: Vec<ScheduledArtifactRejection>,
    /// Folded-case keys of every accepted and refused name, so one path cannot be declared twice
    /// under two spellings that a case-insensitive or non-normalized reader would confuse.
    names: HashSet<String>,
    total: u64,
    /// Entries the walk may still examine; zero stops it at the next candidate.
    candidates_left: usize,
    /// Refusals observed after the list bound was reached; only their count is kept.
    omitted_rejections: u32,
    /// Whether the walk stopped at the candidate bound, which the manifest states separately so a
    /// full refusal list cannot hide that candidates were never examined.
    walk_truncated: bool,
    /// Whether the walk already stopped, so a parent level does not resume after a nested one.
    stopped: bool,
}

impl Collector {
    /// Starts a walk that has not examined or refused anything yet.
    fn new() -> Self {
        Self {
            items: Vec::new(),
            rejections: Vec::new(),
            names: HashSet::new(),
            total: 0,
            candidates_left: MAX_WALKED_CANDIDATES,
            omitted_rejections: 0,
            walk_truncated: false,
            stopped: false,
        }
    }

    /// Walks one directory level at a time without following links, collecting accepted files.
    ///
    /// The candidate budget is spent while the directory is read, before one level is kept or
    /// sorted: a directory holding tens of thousands of names costs no more memory, and no more
    /// entries read, than the walk may still examine, so the terminal write cannot be delayed by a
    /// directory an agent flooded.
    fn visit(&mut self, directory: &Path, prefix: &str) {
        if self.stopped {
            return;
        }
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
        let bounded = bounded_entries(entries, self.candidates_left);
        let mut entries = Vec::with_capacity(bounded.entries.len());
        for entry in bounded.entries {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    self.refuse(
                        directory_name(prefix),
                        ScheduledArtifactRejectionReason::ReadFailed,
                    );
                    return;
                }
            }
        }
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if self.stopped {
                return;
            }
            if self.candidates_left == 0 {
                // A nested level spent the rest of the budget after this one was read, so neither
                // the names left here nor those below them were examined.
                self.stop(prefix);
                return;
            }
            self.candidates_left -= 1;
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
        if bounded.truncated {
            // This level held more names than the walk could still examine, so the rest were never
            // read and the batch that was read is all this level contributes.
            self.stop(prefix);
        }
    }

    /// Stops the walk at the candidate bound and states why on its own.
    ///
    /// The stop is a fact about the walk rather than a refused candidate, so it is never folded into
    /// `omitted_rejections`: that count stays a summary of candidates the walk refuted, while
    /// `walk_truncated` says an unknown number was never examined. Its detail names the level the
    /// walk stopped in when the refusal list still has room; a full list hides the name, not the fact.
    fn stop(&mut self, prefix: &str) {
        self.stopped = true;
        self.walk_truncated = true;
        if self.rejections.len() < MAX_SCHEDULED_ARTIFACT_REJECTIONS {
            self.rejections.push(ScheduledArtifactRejection {
                display_name: bounded_name(directory_name(prefix)),
                reason: ScheduledArtifactRejectionReason::CandidateLimit,
            });
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
    ///
    /// The list is bounded so a directory full of unusable names cannot build a terminal result
    /// larger than the frame that carries it. A refusal past the bound is still counted, and the
    /// manifest states how many were dropped instead of hiding them.
    fn refuse(&mut self, display_name: String, reason: ScheduledArtifactRejectionReason) {
        if self.rejections.len() >= MAX_SCHEDULED_ARTIFACT_REJECTIONS {
            self.omitted_rejections = self.omitted_rejections.saturating_add(1);
            return;
        }
        self.rejections.push(ScheduledArtifactRejection {
            display_name: bounded_name(display_name),
            reason,
        });
    }
}

/// Keeps a refused name inside the wire bound, cutting on a character boundary.
///
/// Only a name that is already outside the collector's path limits can reach this: accepted paths
/// are bounded by `StrictRelativePath`, while a name the platform cannot decode arrives through a
/// lossy conversion and is bounded here.
fn bounded_name(name: String) -> String {
    if name.len() <= MAX_SCHEDULED_ARTIFACT_REJECTION_NAME_BYTES {
        return name;
    }
    let mut end = MAX_SCHEDULED_ARTIFACT_REJECTION_NAME_BYTES;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
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
