use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};

pub const PROJECT_WORKSPACE: &str = ".flameframe";
const GITIGNORE_ENTRY: &str = ".flameframe/";

pub fn ensure_project_gitignore_for(path: &Path) -> Result<()> {
    let Some(workspace) = project_workspace_root(path) else {
        return Ok(());
    };
    let project_root = workspace.parent().unwrap_or_else(|| Path::new("."));
    let gitignore = project_root.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", gitignore.display()));
        }
    };

    if existing.lines().any(|line| line.trim() == GITIGNORE_ENTRY) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)
        .with_context(|| format!("failed to open {}", gitignore.display()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file).with_context(|| format!("failed to update {}", gitignore.display()))?;
    }
    writeln!(file, "{GITIGNORE_ENTRY}")
        .with_context(|| format!("failed to update {}", gitignore.display()))?;

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
    fn writes_workspace_rule_to_project_gitignore_once() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-test-{unique}"));
        fs::create_dir_all(&root)?;
        let work_dir = root.join(PROJECT_WORKSPACE).join("downloads");
        let gitignore = root.join(".gitignore");
        fs::write(&gitignore, "target/\n")?;

        ensure_project_gitignore_for(&work_dir)?;
        ensure_project_gitignore_for(&work_dir)?;

        assert_eq!(fs::read_to_string(&gitignore)?, "target/\n.flameframe/\n");
        assert!(!work_dir.join(".gitignore").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
