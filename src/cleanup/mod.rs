pub mod execute;

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::error::{DevcleanError, Result};

use crate::artifact::recovery::{RecoveryInfo, RecoveryStatus, recovery_for};
use crate::artifact::{ArtifactKind, Finding};
use crate::fs::ScanResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupType {
    NodeModules,
    Target,
    Next,
    Nuxt,
    Gradle,
    DotVenv,
    Venv,
    Pycache,
    Dist,
    Build,
}

impl CleanupType {
    pub const ALL: [CleanupType; 10] = [
        Self::NodeModules,
        Self::Target,
        Self::Next,
        Self::Nuxt,
        Self::Gradle,
        Self::DotVenv,
        Self::Venv,
        Self::Pycache,
        Self::Dist,
        Self::Build,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::NodeModules => "node_modules",
            Self::Target => "target",
            Self::Next => ".next",
            Self::Nuxt => ".nuxt",
            Self::Gradle => ".gradle",
            Self::DotVenv => ".venv",
            Self::Venv => "venv",
            Self::Pycache => "__pycache__",
            Self::Dist => "dist",
            Self::Build => "build",
        }
    }

    fn matches(self, finding: &Finding) -> bool {
        match self {
            Self::NodeModules => finding.kind == ArtifactKind::NodeModules,
            Self::Target => finding.kind == ArtifactKind::RustTarget,
            Self::Next => finding.kind == ArtifactKind::NextBuild,
            Self::Nuxt => finding.kind == ArtifactKind::NuxtBuild,
            Self::Gradle => finding.kind == ArtifactKind::GradleCache,
            Self::DotVenv => {
                finding.kind == ArtifactKind::PythonVenv
                    && finding.path.file_name().is_some_and(|name| name == ".venv")
            }
            Self::Venv => {
                finding.kind == ArtifactKind::PythonVenv
                    && finding.path.file_name().is_some_and(|name| name == "venv")
            }
            Self::Pycache => finding.kind == ArtifactKind::PythonPycache,
            Self::Dist => finding.kind == ArtifactKind::Dist,
            Self::Build => finding.kind == ArtifactKind::Build,
        }
    }
}

impl FromStr for CleanupType {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "node_modules" => Ok(Self::NodeModules),
            "target" => Ok(Self::Target),
            ".next" => Ok(Self::Next),
            ".nuxt" => Ok(Self::Nuxt),
            ".gradle" => Ok(Self::Gradle),
            ".venv" => Ok(Self::DotVenv),
            "venv" => Ok(Self::Venv),
            "__pycache__" => Ok(Self::Pycache),
            "dist" => Ok(Self::Dist),
            "build" => Ok(Self::Build),
            _ => Err(format!(
                "unknown artifact type '{value}'. Supported types: {}",
                supported_type_names_display()
            )),
        }
    }
}

impl fmt::Display for CleanupType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPlan {
    pub entries: Vec<CleanupPlanEntry>,
}

impl CleanupPlan {
    pub fn planned_bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| entry.finding.size_bytes)
            .sum()
    }

    pub fn executable_entries(&self) -> impl Iterator<Item = &CleanupPlanEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.block_reason.is_none())
    }

    pub fn blocked_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.block_reason.is_some())
            .count()
    }

    pub fn has_unsafe_entries(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.block_reason == Some(BlockReason::UnsafePath))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPlanEntry {
    pub finding: Finding,
    pub recovery: RecoveryInfo,
    pub block_reason: Option<BlockReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    ConditionalRecovery,
    UnknownRecovery,
    UnsafePath,
}

pub fn supported_type_names_display() -> String {
    CleanupType::ALL
        .iter()
        .map(|t| t.name())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) struct CleanupRoots {
    pub(crate) scan_root: PathBuf,
    pub(crate) home_dir: Option<PathBuf>,
}

impl CleanupRoots {
    pub(crate) fn resolve(scan_root: &Path, home_dir: Option<&Path>) -> Result<Self> {
        let scan_root = scan_root
            .canonicalize()
            .map_err(|e| DevcleanError::io(scan_root, e))?;
        let home_dir = home_dir.and_then(|path| path.canonicalize().ok());

        Ok(Self {
            scan_root,
            home_dir,
        })
    }
}

pub fn build_cleanup_plan(
    scan: &ScanResult,
    selected_types: &[CleanupType],
    scan_root: &Path,
    home_dir: Option<&Path>,
) -> CleanupPlan {
    let roots = CleanupRoots::resolve(scan_root, home_dir).ok();
    let entries = scan
        .findings
        .iter()
        .filter(|finding| {
            selected_types
                .iter()
                .any(|selected_type| selected_type.matches(finding))
        })
        .cloned()
        .map(|finding| {
            let recovery = recovery_for(finding.kind, &finding.path);
            let block_reason = block_reason(&finding, &recovery, roots.as_ref());
            CleanupPlanEntry {
                finding,
                recovery,
                block_reason,
            }
        })
        .collect();

    CleanupPlan { entries }
}

fn block_reason(
    finding: &Finding,
    recovery: &RecoveryInfo,
    roots: Option<&CleanupRoots>,
) -> Option<BlockReason> {
    match recovery.status {
        RecoveryStatus::Regenerable => {}
        RecoveryStatus::Conditional => return Some(BlockReason::ConditionalRecovery),
        RecoveryStatus::Unknown => return Some(BlockReason::UnknownRecovery),
    }

    let Some(roots) = roots else {
        return Some(BlockReason::UnsafePath);
    };
    validate_cleanup_path(&finding.path, roots)
        .err()
        .map(|_| BlockReason::UnsafePath)
}

pub(crate) fn validate_cleanup_path(path: &Path, roots: &CleanupRoots) -> Result<()> {
    let candidate = path
        .canonicalize()
        .map_err(|e| DevcleanError::io(path, e))?;

    if candidate == roots.scan_root {
        return Err(DevcleanError::msg("refusing to remove the scan root"));
    }

    if is_filesystem_root(&candidate) {
        return Err(DevcleanError::msg("refusing to remove a filesystem root"));
    }

    if let Some(home_dir) = &roots.home_dir
        && candidate == *home_dir
    {
        return Err(DevcleanError::msg("refusing to remove the home directory"));
    }

    if candidate.strip_prefix(&roots.scan_root).is_err() {
        return Err(DevcleanError::msg("path is outside the scan root"));
    }

    let metadata = path
        .symlink_metadata()
        .map_err(|e| DevcleanError::io(path, e))?;
    if metadata.file_type().is_symlink() {
        return Err(DevcleanError::msg(
            "refusing to remove symlink artifact path",
        ));
    }
    if !metadata.is_dir() {
        return Err(DevcleanError::msg("planned artifact is not a directory"));
    }

    Ok(())
}

fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::fs::scan_path;

    use super::*;

    #[test]
    fn parses_stable_cli_type_names() {
        assert_eq!(
            "node_modules".parse::<CleanupType>().unwrap(),
            CleanupType::NodeModules
        );
        assert_eq!(
            "target".parse::<CleanupType>().unwrap(),
            CleanupType::Target
        );
        assert_eq!(
            ".venv".parse::<CleanupType>().unwrap(),
            CleanupType::DotVenv
        );
        assert!("unknown".parse::<CleanupType>().is_err());
    }

    #[test]
    fn plan_filters_multiple_types_without_deleting() {
        let tmp = tempdir().unwrap();
        let project = fixture_project(tmp.path());
        let scan = scan_path(tmp.path()).unwrap();

        let plan = build_cleanup_plan(
            &scan,
            &[CleanupType::NodeModules, CleanupType::Target],
            tmp.path(),
            None,
        );

        assert_eq!(plan.entries.len(), 2);
        assert!(project.join("node_modules").exists());
        assert!(project.join("target").exists());
    }

    #[test]
    fn conditional_and_unknown_recovery_are_blocked() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("app");
        fs::create_dir_all(project.join(".venv").join("bin")).unwrap();
        fs::write(project.join(".venv").join("pyvenv.cfg"), "").unwrap();
        fs::write(project.join(".venv").join("bin").join("python"), "").unwrap();
        fs::write(project.join("requirements.txt"), "").unwrap();

        let scan = scan_path(tmp.path()).unwrap();
        let plan = build_cleanup_plan(&scan, &[CleanupType::DotVenv], tmp.path(), None);

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].block_reason,
            Some(BlockReason::ConditionalRecovery)
        );
    }

    #[test]
    fn unknown_recovery_is_blocked() {
        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("dist");
        fs::create_dir_all(&artifact).unwrap();

        let scan = ScanResult {
            findings: vec![Finding {
                path: artifact,
                kind: ArtifactKind::Dist,
                size_bytes: 10,
                modified_at: None,
                identity: None,
            }],
            stats: Default::default(),
        };

        let plan = build_cleanup_plan(&scan, &[CleanupType::Dist], tmp.path(), None);

        assert_eq!(
            plan.entries[0].block_reason,
            Some(BlockReason::UnknownRecovery)
        );
    }

    #[test]
    fn root_boundary_validation_blocks_outside_paths() {
        let tmp = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let artifact = outside.path().join("node_modules");
        fs::create_dir_all(&artifact).unwrap();

        let scan = ScanResult {
            findings: vec![Finding {
                path: artifact,
                kind: ArtifactKind::NodeModules,
                size_bytes: 10,
                modified_at: None,
                identity: None,
            }],
            stats: Default::default(),
        };

        let plan = build_cleanup_plan(&scan, &[CleanupType::NodeModules], tmp.path(), None);

        assert_eq!(plan.entries[0].block_reason, Some(BlockReason::UnsafePath));
    }

    fn fixture_project(root: &Path) -> std::path::PathBuf {
        let project = root.join("app");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::create_dir_all(project.join("target")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
        fs::write(project.join("node_modules").join("dep.js"), "dep").unwrap();
        fs::write(project.join("target").join("app"), "bin").unwrap();
        project
    }
}
