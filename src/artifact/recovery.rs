use std::path::Path;

use crate::artifact::{ArtifactKind, has_any_file, has_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    Regenerable,
    Conditional,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInfo {
    pub status: RecoveryStatus,
    pub restore_hint: Option<&'static str>,
}

impl RecoveryInfo {
    pub fn display_text(&self) -> &'static str {
        if let Some(hint) = self.restore_hint {
            return hint;
        }

        match self.status {
            RecoveryStatus::Regenerable => "regenerable",
            RecoveryStatus::Conditional => "conditional",
            RecoveryStatus::Unknown => "unknown",
        }
    }
}

pub fn recovery_for(kind: ArtifactKind, artifact_path: &Path) -> RecoveryInfo {
    let project_dir = artifact_path.parent();

    match kind {
        ArtifactKind::NodeModules => RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some(node_install_hint(project_dir)),
        },
        ArtifactKind::RustTarget => RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some("cargo build"),
        },
        ArtifactKind::GradleCache => RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some("project Gradle state"),
        },
        ArtifactKind::PythonVenv => python_venv_recovery(project_dir),
        ArtifactKind::PythonPycache => RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some("automatic"),
        },
        ArtifactKind::NextBuild | ArtifactKind::NuxtBuild => RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some(node_build_hint(project_dir)),
        },
        ArtifactKind::Dist | ArtifactKind::Build => build_output_recovery(project_dir),
    }
}

fn node_install_hint(project_dir: Option<&Path>) -> &'static str {
    let Some(project_dir) = project_dir else {
        return "npm install";
    };

    if has_file(project_dir, "pnpm-lock.yaml") {
        "pnpm install"
    } else if has_file(project_dir, "yarn.lock") {
        "yarn install"
    } else {
        "npm install"
    }
}

fn node_build_hint(project_dir: Option<&Path>) -> &'static str {
    let Some(project_dir) = project_dir else {
        return "package build";
    };

    if has_file(project_dir, "pnpm-lock.yaml") {
        "pnpm build"
    } else if has_file(project_dir, "yarn.lock") {
        "yarn build"
    } else if has_file(project_dir, "package-lock.json") || has_file(project_dir, "package.json") {
        "npm run build"
    } else {
        "package build"
    }
}

fn python_venv_recovery(project_dir: Option<&Path>) -> RecoveryInfo {
    let Some(project_dir) = project_dir else {
        return RecoveryInfo {
            status: RecoveryStatus::Unknown,
            restore_hint: None,
        };
    };

    if has_any_file(
        project_dir,
        &[
            "requirements.txt",
            "pyproject.toml",
            "poetry.lock",
            "Pipfile",
            "Pipfile.lock",
            "environment.yml",
            "environment.yaml",
        ],
    ) {
        RecoveryInfo {
            status: RecoveryStatus::Conditional,
            restore_hint: Some("recreate env"),
        }
    } else {
        RecoveryInfo {
            status: RecoveryStatus::Unknown,
            restore_hint: None,
        }
    }
}

fn build_output_recovery(project_dir: Option<&Path>) -> RecoveryInfo {
    let Some(project_dir) = project_dir else {
        return RecoveryInfo {
            status: RecoveryStatus::Unknown,
            restore_hint: None,
        };
    };

    if has_file(project_dir, "package.json") {
        RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some(node_build_hint(Some(project_dir))),
        }
    } else if has_any_file(
        project_dir,
        &[
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradlew",
        ],
    ) {
        RecoveryInfo {
            status: RecoveryStatus::Regenerable,
            restore_hint: Some("gradle build"),
        }
    } else {
        RecoveryInfo {
            status: RecoveryStatus::Unknown,
            restore_hint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn detects_node_package_manager_restore_hints() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("app");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::write(project.join("pnpm-lock.yaml"), "").unwrap();

        let recovery = recovery_for(ArtifactKind::NodeModules, &project.join("node_modules"));

        assert_eq!(recovery.status, RecoveryStatus::Regenerable);
        assert_eq!(recovery.restore_hint, Some("pnpm install"));
    }

    #[test]
    fn defaults_node_modules_to_npm_install() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("app");
        fs::create_dir_all(project.join("node_modules")).unwrap();

        let recovery = recovery_for(ArtifactKind::NodeModules, &project.join("node_modules"));

        assert_eq!(recovery.restore_hint, Some("npm install"));
    }

    #[test]
    fn python_venv_is_conditional_when_dependency_metadata_exists() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("python-app");
        fs::create_dir_all(project.join(".venv")).unwrap();
        fs::write(project.join("pyproject.toml"), "").unwrap();

        let recovery = recovery_for(ArtifactKind::PythonVenv, &project.join(".venv"));

        assert_eq!(recovery.status, RecoveryStatus::Conditional);
        assert_eq!(recovery.restore_hint, Some("recreate env"));
    }

    #[test]
    fn python_venv_without_dependency_metadata_is_unknown() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("python-app");
        fs::create_dir_all(project.join(".venv")).unwrap();

        let recovery = recovery_for(ArtifactKind::PythonVenv, &project.join(".venv"));

        assert_eq!(recovery.status, RecoveryStatus::Unknown);
        assert_eq!(recovery.restore_hint, None);
    }

    #[test]
    fn ambiguous_build_and_dist_recovery_stays_unknown() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("notes");
        fs::create_dir_all(project.join("build")).unwrap();
        fs::create_dir_all(project.join("dist")).unwrap();

        assert_eq!(
            recovery_for(ArtifactKind::Build, &project.join("build")).status,
            RecoveryStatus::Unknown
        );
        assert_eq!(
            recovery_for(ArtifactKind::Dist, &project.join("dist")).status,
            RecoveryStatus::Unknown
        );
    }
}
