use std::path::Path;

use crate::artifact::{ArtifactKind, has_any_file, has_file};

pub fn detect_artifact(path: &Path) -> Option<ArtifactKind> {
    let name = path.file_name()?.to_str()?;

    match name {
        "node_modules" if has_file(parent(path)?, "package.json") => {
            Some(ArtifactKind::NodeModules)
        }
        "target" if has_file(parent(path)?, "Cargo.toml") => Some(ArtifactKind::RustTarget),
        ".gradle" if has_gradle_context(parent(path)?) => Some(ArtifactKind::GradleCache),
        ".venv" | "venv" if looks_like_python_venv(path) => Some(ArtifactKind::PythonVenv),
        "__pycache__" => Some(ArtifactKind::PythonPycache),
        ".next" if has_file(parent(path)?, "package.json") => Some(ArtifactKind::NextBuild),
        ".nuxt" if has_file(parent(path)?, "package.json") => Some(ArtifactKind::NuxtBuild),
        "dist" if has_node_context(parent(path)?) => Some(ArtifactKind::Dist),
        "build" if has_build_context(parent(path)?) => Some(ArtifactKind::Build),
        _ => None,
    }
}

fn parent(path: &Path) -> Option<&Path> {
    path.parent()
}

fn has_node_context(dir: &Path) -> bool {
    has_file(dir, "package.json")
}

fn has_gradle_context(dir: &Path) -> bool {
    has_any_file(
        dir,
        &[
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradlew",
        ],
    )
}

fn has_build_context(dir: &Path) -> bool {
    has_node_context(dir) || has_gradle_context(dir)
}

fn looks_like_python_venv(path: &Path) -> bool {
    if !has_file(path, "pyvenv.cfg") {
        return false;
    }

    let unix_python =
        path.join("bin").join("python").is_file() || path.join("bin").join("python3").is_file();
    let windows_python = path.join("Scripts").join("python.exe").is_file()
        || path.join("Scripts").join("activate").is_file();

    unix_python || windows_python || path.join("bin").join("activate").is_file()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn detects_contextual_artifacts() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("app");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();

        assert_eq!(
            detect_artifact(&project.join("node_modules")),
            Some(ArtifactKind::NodeModules)
        );
    }

    #[test]
    fn rejects_node_modules_without_owning_package_json() {
        let tmp = tempdir().unwrap();
        let node_modules = tmp.path().join("cache").join("node_modules");
        fs::create_dir_all(&node_modules).unwrap();

        assert_eq!(detect_artifact(&node_modules), None);
    }

    #[test]
    fn rejects_generic_names_without_context() {
        let tmp = tempdir().unwrap();
        let target = tmp.path().join("notes").join("target");
        let build = tmp.path().join("photos").join("build");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&build).unwrap();

        assert_eq!(detect_artifact(&target), None);
        assert_eq!(detect_artifact(&build), None);
    }

    #[test]
    fn detects_python_virtual_environment_by_structure() {
        let tmp = tempdir().unwrap();
        let venv = tmp.path().join("space project").join(".venv");
        fs::create_dir_all(venv.join("bin")).unwrap();
        fs::write(venv.join("pyvenv.cfg"), "").unwrap();
        fs::write(venv.join("bin").join("python"), "").unwrap();

        assert_eq!(detect_artifact(&venv), Some(ArtifactKind::PythonVenv));
    }

    #[test]
    fn rejects_venv_name_without_virtual_environment_files() {
        let tmp = tempdir().unwrap();
        let venv = tmp.path().join("venv");
        fs::create_dir_all(venv.join("bin")).unwrap();
        fs::write(venv.join("bin").join("python"), "").unwrap();

        assert_eq!(detect_artifact(&venv), None);
    }

    #[test]
    fn rejects_build_with_only_rust_context() {
        let tmp = tempdir().unwrap();
        let project = tmp.path().join("crate");
        let build = project.join("build");
        fs::create_dir_all(&build).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = \"crate\"\n").unwrap();

        assert_eq!(detect_artifact(&build), None);
    }
}
