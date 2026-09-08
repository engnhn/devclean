pub mod detect;
pub mod recovery;

use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArtifactKind {
    NodeModules,
    RustTarget,
    GradleCache,
    PythonVenv,
    PythonPycache,
    NextBuild,
    NuxtBuild,
    Dist,
    Build,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::NodeModules => "node_modules",
            Self::RustTarget => "target",
            Self::GradleCache => ".gradle",
            Self::PythonVenv => "venv",
            Self::PythonPycache => "__pycache__",
            Self::NextBuild => ".next",
            Self::NuxtBuild => ".nuxt",
            Self::Dist => "dist",
            Self::Build => "build",
        }
    }
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub identity: Option<FileIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
    pub changed_seconds: i64,
    pub changed_nanoseconds: i64,
}

pub(crate) fn has_file(dir: &std::path::Path, name: &str) -> bool {
    dir.join(name).is_file()
}

pub(crate) fn has_any_file(dir: &std::path::Path, names: &[&str]) -> bool {
    names.iter().any(|name| has_file(dir, name))
}
