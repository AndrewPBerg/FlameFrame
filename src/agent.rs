use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow};

use crate::cli::{AgentInstallArgs, AgentKind};

const PI_EXTENSION: &str = include_str!("../agents/pi/flameframe.ts");

const SKILL: &str = r#"---
name: flameframe
description: Process local video files and video URLs into compact, timestamped evidence packs with FlameFrame. Use when the user asks to inspect, summarize, search, or answer questions about a video.
---

# FlameFrame

Use FlameFrame to turn a video URL or local video into agent-readable evidence before drawing conclusions from it.

## Workflow

1. Process the video into a deterministic work directory:

   ```sh
   flameframe process <URL_OR_VIDEO> --work-dir .flameframe/<slug>
   ```

   When installed for Pi, use the `flameframe_process` tool instead; it runs this
   workflow and registers the completed pack with the session browser.

2. Read `<work-dir>/video.context.md` first when it exists.
3. Read `<work-dir>/inspect.visual.context.md` next.
4. Open selected frames only when the markdown evidence is insufficient.
5. Request closer evidence around a timestamp when needed:

   ```sh
   flameframe zoom <work-dir>/video.mp4 --at <TIMESTAMP> --out <work-dir>/zooms/<timestamp>
   ```

Cite timestamps and distinguish transcript evidence from visual evidence. Do not infer details that the evidence pack does not support.
"#;

pub fn install(args: &AgentInstallArgs) -> Result<()> {
    let kind = args.kind()?;
    let root = install_root(args.project)?;
    let skill_path = root.join(relative_skill_path(kind, args.project));
    write_install_file(&skill_path, SKILL, "skill")?;

    let extension_path = relative_extension_path(kind, args.project).map(|path| root.join(path));
    if let Some(path) = &extension_path {
        write_install_file(path, PI_EXTENSION, "extension")?;
    }

    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Installed FlameFrame skill: {}", skill_path.display())
        .context("failed to write agent-install result")?;
    if let Some(path) = extension_path {
        writeln!(stdout, "Installed FlameFrame Pi extension: {}", path.display())
            .context("failed to write agent-install result")?;
    }
    Ok(())
}

fn install_root(project: bool) -> Result<PathBuf> {
    if project {
        env::current_dir().context("failed to determine the current project directory")
    } else {
        home_dir()
    }
}

fn write_install_file(path: &PathBuf, content: &str, kind: &str) -> Result<()> {
    let parent = path.parent().context("installation path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {kind} directory {}", parent.display()))?;
    fs::write(path, content)
        .with_context(|| format!("failed to write FlameFrame {kind} {}", path.display()))
}

const fn relative_skill_path(kind: AgentKind, project: bool) -> &'static str {
    match (kind, project) {
        (AgentKind::Claude, _) => ".claude/skills/flameframe/SKILL.md",
        (AgentKind::Codex, false) => ".codex/skills/flameframe/SKILL.md",
        (AgentKind::Codex, true) | (AgentKind::Agents, _) => ".agents/skills/flameframe/SKILL.md",
        (AgentKind::Pi, false) => ".pi/agent/skills/flameframe/SKILL.md",
        (AgentKind::Pi, true) => ".pi/skills/flameframe/SKILL.md",
    }
}

const fn relative_extension_path(kind: AgentKind, project: bool) -> Option<&'static str> {
    match (kind, project) {
        (AgentKind::Pi, false) => Some(".pi/agent/extensions/flameframe.ts"),
        (AgentKind::Pi, true) => Some(".pi/extensions/flameframe.ts"),
        _ => None,
    }
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("could not determine the home directory; set HOME or USERPROFILE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_extension_registers_the_process_tool() {
        assert!(PI_EXTENSION.contains("pi.registerTool({"));
        assert!(PI_EXTENSION.contains("name: PROCESS_TOOL_NAME"));
    }

    #[test]
    fn selects_expected_skill_paths() {
        assert_eq!(
            relative_skill_path(AgentKind::Claude, false),
            ".claude/skills/flameframe/SKILL.md"
        );
        assert_eq!(
            relative_skill_path(AgentKind::Codex, true),
            ".agents/skills/flameframe/SKILL.md"
        );
        assert_eq!(
            relative_skill_path(AgentKind::Pi, false),
            ".pi/agent/skills/flameframe/SKILL.md"
        );
    }
}
