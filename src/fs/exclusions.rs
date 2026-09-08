use std::path::{Component, Path};

pub fn should_exclude_dir(path: &Path) -> bool {
    contains_component_sequence(path, &[".local", "share", "Trash"])
        || contains_component_sequence(path, &[".npm", "_npx"])
        || contains_component_sequence(path, &[".nvm", "versions", "node"])
}

fn contains_component_sequence(path: &Path, sequence: &[&str]) -> bool {
    let mut matched = 0;

    for component in path.components() {
        let Component::Normal(value) = component else {
            matched = 0;
            continue;
        };
        let Some(value) = value.to_str() else {
            matched = 0;
            continue;
        };

        if value == sequence[matched] {
            matched += 1;
            if matched == sequence.len() {
                return true;
            }
        } else {
            matched = usize::from(value == sequence[0]);
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn excludes_only_recognized_infrastructure_structures() {
        assert!(should_exclude_dir(Path::new(
            "/home/alex/.npm/_npx/123/node_modules"
        )));
        assert!(should_exclude_dir(Path::new(
            "/home/alex/.nvm/versions/node/v22.0.0/lib"
        )));
        assert!(should_exclude_dir(Path::new(
            "/home/alex/.local/share/Trash/files/project"
        )));
    }

    #[test]
    fn does_not_exclude_similar_project_names_by_basename_only() {
        assert!(!should_exclude_dir(Path::new(
            "/work/app/.npm-cache/project/node_modules"
        )));
        assert!(!should_exclude_dir(Path::new(
            "/work/nvm/project/node_modules"
        )));
        assert!(!should_exclude_dir(Path::new(
            "/work/app/.local/share-not-trash/node_modules"
        )));
    }
}
