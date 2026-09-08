pub mod exclusions;
pub mod file_id;
pub mod usage;

use std::path::Path;
use std::time::SystemTime;

use crate::artifact::Finding;
use crate::artifact::detect::detect_artifact;
use crate::error::{DevcleanError, Result};
use crate::fs::exclusions::should_exclude_dir;
use crate::fs::file_id::file_identity;
use crate::fs::usage::{UsageTracker, directory_usage};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub unreadable_entries: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanResult {
    pub findings: Vec<Finding>,
    pub stats: ScanStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanOptions {
    pub min_age_days: Option<u64>,
    pub now: Option<SystemTime>,
}

pub fn scan_path(root: &Path) -> Result<ScanResult> {
    scan_path_with_options(root, &ScanOptions::default())
}

pub fn scan_path_with_options(root: &Path, options: &ScanOptions) -> Result<ScanResult> {
    let metadata = std::fs::symlink_metadata(root).map_err(|e| DevcleanError::io(root, e))?;

    if !metadata.is_dir() {
        return Err(DevcleanError::msg(format!(
            "scan path must be a directory: {}",
            root.display()
        )));
    }

    let mut result = ScanResult::default();
    let mut usage_tracker = UsageTracker::new();
    let mut stack = vec![root.to_path_buf()];
    let now = options.now.unwrap_or_else(SystemTime::now);

    while let Some(current_path) = stack.pop() {
        let metadata = match current_path.symlink_metadata() {
            Ok(meta) => meta,
            Err(_) => {
                result.stats.unreadable_entries += 1;
                continue;
            }
        };

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        if should_exclude_dir(&current_path) || is_ignored_directory(&current_path) {
            continue;
        }

        if let Some(kind) = detect_artifact(&current_path) {
            let mod_time = modified_at(Some(&metadata));

            if !matches_age_filter(mod_time, options.min_age_days, now) {
                continue;
            }

            let size_bytes = directory_usage(
                &current_path,
                &mut usage_tracker,
                &mut result.stats.unreadable_entries,
            );

            result.findings.push(Finding {
                path: current_path,
                kind,
                size_bytes,
                modified_at: mod_time,
                identity: file_identity(&metadata),
            });
            continue;
        }

        let read_dir = match std::fs::read_dir(&current_path) {
            Ok(rd) => rd,
            Err(_) => {
                result.stats.unreadable_entries += 1;
                continue;
            }
        };

        for entry in read_dir {
            match entry {
                Ok(entry) => stack.push(entry.path()),
                Err(_) => result.stats.unreadable_entries += 1,
            }
        }
    }

    Ok(result)
}

fn is_ignored_directory(dir: &Path) -> bool {
    dir.join(".devcleanignore").is_file()
}

fn matches_age_filter(
    modified_at: Option<SystemTime>,
    min_age_days: Option<u64>,
    now: SystemTime,
) -> bool {
    let Some(min_days) = min_age_days else {
        return true;
    };

    let Some(modified) = modified_at else {
        return false;
    };

    let Ok(age) = now.duration_since(modified) else {
        return false;
    };

    age.as_secs() >= min_days * 24 * 60 * 60
}

fn modified_at(metadata: Option<&std::fs::Metadata>) -> Option<SystemTime> {
    metadata.and_then(|metadata| metadata.modified().ok())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::tempdir;

    use crate::artifact::ArtifactKind;

    use super::*;

    #[test]
    fn scans_recursively_aggregates_and_skips_nested_artifacts() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("Projects ü").join("app with spaces");
        let node_modules = root.join("node_modules");
        let nested_pycache = node_modules.join("dep").join("__pycache__");
        let target = root.join("target");
        let unrelated_target = tmp.path().join("archive").join("target");

        fs::create_dir_all(&nested_pycache).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&unrelated_target).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
        fs::write(node_modules.join("left-pad.js"), vec![0; 10]).unwrap();
        fs::write(nested_pycache.join("mod.pyc"), vec![0; 20]).unwrap();
        fs::write(target.join("app"), vec![0; 30]).unwrap();
        fs::write(unrelated_target.join("note.txt"), vec![0; 40]).unwrap();

        let scan = scan_path(tmp.path()).unwrap();

        assert_eq!(scan.findings.len(), 2);
        assert!(scan.findings.iter().any(|finding| {
            finding.kind == ArtifactKind::NodeModules && finding.size_bytes >= 30
        }));
        assert!(scan.findings.iter().any(|finding| {
            finding.kind == ArtifactKind::RustTarget && finding.size_bytes >= 30
        }));
        assert!(
            scan.findings
                .iter()
                .all(|finding| finding.modified_at.is_some())
        );
    }

    #[test]
    fn excludes_package_manager_runtime_and_trash_internals() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();

        let npm_npx = home
            .join(".npm")
            .join("_npx")
            .join("abc")
            .join("node_modules");
        let nvm_global_package = home
            .join(".nvm")
            .join("versions")
            .join("node")
            .join("v22.0.0")
            .join("lib")
            .join("node_modules")
            .join("typescript")
            .join("node_modules");
        let trashed_project = home
            .join(".local")
            .join("share")
            .join("Trash")
            .join("files")
            .join("old-app")
            .join("node_modules");

        fs::create_dir_all(&npm_npx).unwrap();
        fs::create_dir_all(&nvm_global_package).unwrap();
        fs::create_dir_all(&trashed_project).unwrap();
        fs::write(npm_npx.parent().unwrap().join("package.json"), "{}").unwrap();
        fs::write(
            nvm_global_package.parent().unwrap().join("package.json"),
            "{}",
        )
        .unwrap();
        fs::write(trashed_project.parent().unwrap().join("package.json"), "{}").unwrap();

        let scan = scan_path(home).unwrap();

        assert!(scan.findings.is_empty());
    }

    #[test]
    fn skips_directories_containing_devcleanignore() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        let protected_project = home.join("protected-app");

        fs::create_dir_all(protected_project.join("node_modules")).unwrap();
        fs::write(protected_project.join("package.json"), "{}").unwrap();
        fs::write(protected_project.join(".devcleanignore"), "").unwrap();

        let scan = scan_path(home).unwrap();

        assert!(scan.findings.is_empty());
    }

    #[test]
    fn filters_by_min_age_days() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        let project = home.join("old-app");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();

        let now = SystemTime::now();
        let scan_old = scan_path_with_options(
            home,
            &ScanOptions {
                min_age_days: Some(30),
                now: Some(now + Duration::from_secs(40 * 24 * 60 * 60)),
            },
        )
        .unwrap();

        let scan_recent = scan_path_with_options(
            home,
            &ScanOptions {
                min_age_days: Some(30),
                now: Some(now + Duration::from_secs(5 * 24 * 60 * 60)),
            },
        )
        .unwrap();

        assert_eq!(scan_old.findings.len(), 1);
        assert!(scan_recent.findings.is_empty());
    }
}
