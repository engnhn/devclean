use std::collections::HashSet;
use std::fs::Metadata;
use std::path::Path;

use crate::artifact::FileIdentity;
use crate::fs::file_id::file_identity;

#[derive(Debug, Default)]
pub struct UsageTracker {
    seen: HashSet<FileIdentity>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_metadata(&mut self, metadata: &Metadata) -> u64 {
        let identity = file_identity(metadata);
        if let Some(identity) = identity
            && !self.seen.insert(identity)
        {
            return 0;
        }

        allocated_size(metadata)
    }
}

pub fn directory_usage(
    path: &Path,
    tracker: &mut UsageTracker,
    unreadable_entries: &mut usize,
) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(current_path) = stack.pop() {
        let metadata = match current_path.symlink_metadata() {
            Ok(meta) => meta,
            Err(_) => {
                *unreadable_entries += 1;
                continue;
            }
        };

        total += tracker.add_metadata(&metadata);

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(&current_path) {
            Ok(read_dir) => read_dir,
            Err(_) => {
                *unreadable_entries += 1;
                continue;
            }
        };

        for entry in entries {
            match entry {
                Ok(entry) => stack.push(entry.path()),
                Err(_) => *unreadable_entries += 1,
            }
        }
    }

    total
}

#[cfg(unix)]
fn allocated_size(metadata: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_size(metadata: &Metadata) -> u64 {
    metadata.len()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn includes_nested_directories_and_many_small_files() {
        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("artifact");
        let nested = artifact.join("nested");
        fs::create_dir_all(&nested).unwrap();

        for index in 0..64 {
            fs::write(nested.join(format!("file-{index}")), b"x").unwrap();
        }

        let mut tracker = UsageTracker::new();
        let mut unreadable = 0;
        let usage = directory_usage(&artifact, &mut tracker, &mut unreadable);

        assert_eq!(unreadable, 0);
        assert!(usage >= 64);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sparse_file_uses_allocated_blocks_not_logical_length() {
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};

        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("artifact");
        fs::create_dir_all(&artifact).unwrap();

        let sparse = artifact.join("sparse.bin");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&sparse)
            .unwrap();
        file.seek(SeekFrom::Start(1024 * 1024)).unwrap();
        file.write_all(b"x").unwrap();
        drop(file);

        let logical_len = fs::metadata(&sparse).unwrap().len();
        let mut tracker = UsageTracker::new();
        let mut unreadable = 0;
        let usage = directory_usage(&artifact, &mut tracker, &mut unreadable);

        assert_eq!(unreadable, 0);
        assert!(usage < logical_len);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks_to_targets() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("artifact");
        let outside = tmp.path().join("outside.bin");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(&outside, vec![0; 1024 * 1024]).unwrap();
        symlink(&outside, artifact.join("linked-outside.bin")).unwrap();

        let outside_blocks = fs::symlink_metadata(&outside).unwrap().blocks() * 512;
        let mut tracker = UsageTracker::new();
        let mut unreadable = 0;
        let usage = directory_usage(&artifact, &mut tracker, &mut unreadable);

        assert_eq!(unreadable, 0);
        assert!(usage < outside_blocks);
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_are_counted_once() {
        let tmp = tempdir().unwrap();
        let artifact = tmp.path().join("artifact");
        let original = artifact.join("original.bin");
        let linked = artifact.join("linked.bin");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(&original, vec![0; 8192]).unwrap();
        fs::hard_link(&original, &linked).unwrap();

        let file_blocks = fs::symlink_metadata(&original).unwrap().blocks() * 512;
        let mut tracker = UsageTracker::new();
        let mut unreadable = 0;
        let usage = directory_usage(&artifact, &mut tracker, &mut unreadable);

        assert_eq!(unreadable, 0);
        assert!(usage >= file_blocks);
        assert!(usage < file_blocks * 2);
    }
}
