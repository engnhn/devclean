use std::fs::Metadata;

use crate::artifact::FileIdentity;

#[cfg(unix)]
pub fn file_identity(metadata: &Metadata) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
    })
}

#[cfg(not(unix))]
pub fn file_identity(_metadata: &Metadata) -> Option<FileIdentity> {
    None
}
