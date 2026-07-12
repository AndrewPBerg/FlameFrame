use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};

pub const PROJECT_WORKSPACE: &str = ".flameframe";
const GITIGNORE_CONTENT: &str = "*\n";

pub fn ensure_gitignore_for(path: &Path) -> Result<()> {
    let Some(workspace) = project_workspace_root(path) else {
        return Ok(());
    };

    fs::create_dir_all(&workspace)
        .with_context(|| format!("failed to create {}", workspace.display()))?;
    let gitignore = workspace.join(".gitignore");
    if !gitignore.exists() {
        fs::write(&gitignore, GITIGNORE_CONTENT)
            .with_context(|| format!("failed to write {}", gitignore.display()))?;
    }

    Ok(())
}

fn project_workspace_root(path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    for component in path.components() {
        root.push(component.as_os_str());
        if matches!(component, Component::Normal(name) if name == PROJECT_WORKSPACE) {
            return Some(root);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn detects_relative_project_workspace_root() {
        assert_eq!(
            project_workspace_root(Path::new(".flameframe/downloads/job")),
            Some(PathBuf::from(".flameframe"))
        );
    }

    #[test]
    fn writes_gitignore_inside_project_workspace() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-test-{unique}"));
        let work_dir = root.join(PROJECT_WORKSPACE).join("downloads");

        ensure_gitignore_for(&work_dir)?;

        let gitignore = root.join(PROJECT_WORKSPACE).join(".gitignore");
        assert_eq!(fs::read_to_string(&gitignore)?, GITIGNORE_CONTENT);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
