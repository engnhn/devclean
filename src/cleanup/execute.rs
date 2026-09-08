use std::fs;
use std::path::Path;

use crate::artifact::Finding;
use crate::cleanup::{CleanupPlan, CleanupRoots, validate_cleanup_path};
use crate::error::{DevcleanError, Result};
use crate::fs::file_id::file_identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupExecution {
    pub removed: Vec<Finding>,
    pub failed: Vec<CleanupFailure>,
}

impl CleanupExecution {
    pub fn planned_removed_bytes(&self) -> u64 {
        self.removed.iter().map(|finding| finding.size_bytes).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupFailure {
    pub finding: Finding,
    pub error: String,
}

pub fn execute_cleanup_plan(
    plan: &CleanupPlan,
    scan_root: &Path,
    home_dir: Option<&Path>,
) -> CleanupExecution {
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let roots = match CleanupRoots::resolve(scan_root, home_dir) {
        Ok(roots) => roots,
        Err(error) => {
            return CleanupExecution {
                removed,
                failed: plan
                    .executable_entries()
                    .map(|entry| CleanupFailure {
                        finding: entry.finding.clone(),
                        error: error.to_string(),
                    })
                    .collect(),
            };
        }
    };

    for entry in plan.executable_entries() {
        if let Err(error) = validate_cleanup_path(&entry.finding.path, &roots) {
            failed.push(CleanupFailure {
                finding: entry.finding.clone(),
                error: error.to_string(),
            });
            continue;
        }
        if let Err(error) = validate_same_artifact(&entry.finding) {
            failed.push(CleanupFailure {
                finding: entry.finding.clone(),
                error: error.to_string(),
            });
            continue;
        }

        match remove_artifact_path(&entry.finding.path) {
            Ok(()) => removed.push(entry.finding.clone()),
            Err(error) => failed.push(CleanupFailure {
                finding: entry.finding.clone(),
                error: error.to_string(),
            }),
        }
    }

    CleanupExecution { removed, failed }
}

fn remove_artifact_path(path: &Path) -> Result<()> {
    let metadata = path
        .symlink_metadata()
        .map_err(|e| DevcleanError::io(path, e))?;

    if metadata.file_type().is_symlink() {
        return Err(DevcleanError::msg("path was replaced by a symlink"));
    }

    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|e| DevcleanError::io(path, e))
    } else {
        Err(DevcleanError::msg(
            "planned artifact is no longer a directory",
        ))
    }
}

fn validate_same_artifact(finding: &Finding) -> Result<()> {
    let Some(expected) = finding.identity else {
        return Ok(());
    };

    let metadata = finding
        .path
        .symlink_metadata()
        .map_err(|e| DevcleanError::io(&finding.path, e))?;

    if file_identity(&metadata) != Some(expected) {
        return Err(DevcleanError::msg("planned artifact was replaced"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::cleanup::{CleanupType, build_cleanup_plan};
    use crate::fs::scan_path;

    use super::*;

    #[test]
    fn execution_removes_real_artifact_directories() {
        let tmp = tempdir().unwrap();
        let project = fixture_project(tmp.path());
        let artifact = project.join("node_modules");
        let scan = scan_path(tmp.path()).unwrap();
        let plan = build_cleanup_plan(&scan, &[CleanupType::NodeModules], tmp.path(), None);

        let execution = execute_cleanup_plan(&plan, tmp.path(), None);

        assert_eq!(execution.removed.len(), 1);
        assert!(execution.failed.is_empty());
        assert!(!artifact.exists());
        assert!(project.join("target").exists());
    }

    #[test]
    fn missing_path_between_plan_and_execute_is_reported_as_failure() {
        let tmp = tempdir().unwrap();
        let project = fixture_project(tmp.path());
        let artifact = project.join("node_modules");
        let scan = scan_path(tmp.path()).unwrap();
        let plan = build_cleanup_plan(&scan, &[CleanupType::NodeModules], tmp.path(), None);
        fs::remove_dir_all(&artifact).unwrap();

        let execution = execute_cleanup_plan(&plan, tmp.path(), None);

        assert!(execution.removed.is_empty());
        assert_eq!(execution.failed.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn directory_replacement_between_plan_and_execute_is_rejected() {
        let tmp = tempdir().unwrap();
        let project = fixture_project(tmp.path());
        let artifact = project.join("node_modules");
        let scan = scan_path(tmp.path()).unwrap();
        let plan = build_cleanup_plan(&scan, &[CleanupType::NodeModules], tmp.path(), None);

        fs::remove_dir_all(&artifact).unwrap();
        fs::create_dir_all(&artifact).unwrap();
        fs::write(artifact.join("new-file.js"), "new").unwrap();

        let execution = execute_cleanup_plan(&plan, tmp.path(), None);

        assert!(execution.removed.is_empty());
        assert_eq!(execution.failed.len(), 1);
        assert!(artifact.join("new-file.js").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_replacement_between_plan_and_execute_is_rejected() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let project = fixture_project(tmp.path());
        let artifact = project.join("node_modules");
        let outside = tempdir().unwrap();
        let scan = scan_path(tmp.path()).unwrap();
        let plan = build_cleanup_plan(&scan, &[CleanupType::NodeModules], tmp.path(), None);

        fs::remove_dir_all(&artifact).unwrap();
        symlink(outside.path(), &artifact).unwrap();

        let execution = execute_cleanup_plan(&plan, tmp.path(), None);

        assert!(execution.removed.is_empty());
        assert_eq!(execution.failed.len(), 1);
        assert!(artifact.exists());
        assert!(outside.path().exists());
    }

    fn fixture_project(root: &Path) -> PathBuf {
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
