use std::{
    env, fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    process, thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

pub const PROJECT_WORKSPACE: &str = ".flameframe";
const GITIGNORE_CONTENT: &str = "*\n";
const PROCESS_CACHE_DIR: &str = "flameframe/inspect-cache";
const PROCESS_CACHE_LOCK_TIMEOUT: Duration = Duration::from_secs(3600);

#[derive(Debug)]
pub struct ProcessCache {
    entry: PathBuf,
}

#[derive(Debug)]
pub struct ProcessCacheLock {
    path: PathBuf,
}

impl ProcessCache {
    pub fn for_input(input: &str, variant: &str) -> Result<Self> {
        let identity = cache_identity(input)?;
        let root = env::temp_dir().join(PROCESS_CACHE_DIR);
        Ok(Self::at(&root, &identity, variant))
    }

    pub fn restore(&self, work_dir: &Path) -> Result<bool> {
        if !is_complete_cache_entry(&self.entry)? {
            return Ok(false);
        }
        clear_process_artifacts(work_dir)?;
        copy_process_artifacts(&self.entry, work_dir)?;
        Ok(true)
    }

    pub fn store(&self, work_dir: &Path) -> Result<()> {
        let parent = self.entry.parent().context("cache entry has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create process cache directory {}", parent.display())
        })?;

        let staging = parent.join(format!(
            ".{}.partial-{}-{}",
            self.entry.file_name().and_then(|name| name.to_str()).unwrap_or("entry"),
            process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
        ));
        if staging.exists() {
            fs::remove_dir_all(&staging).with_context(|| {
                format!("failed to remove stale cache staging directory {}", staging.display())
            })?;
        }

        let result = (|| {
            copy_process_artifacts(work_dir, &staging)?;
            if !is_complete_cache_entry(&staging)? {
                bail!("refusing to cache incomplete process output from {}", work_dir.display());
            }
            if self.entry.exists() {
                fs::remove_dir_all(&self.entry).with_context(|| {
                    format!("failed to remove incomplete cache entry {}", self.entry.display())
                })?;
            }
            fs::rename(&staging, &self.entry).with_context(|| {
                format!("failed to publish process cache entry {}", self.entry.display())
            })
        })();

        if staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    pub fn lock(&self) -> Result<ProcessCacheLock> {
        let parent = self.entry.parent().context("cache entry has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create process cache directory {}", parent.display())
        })?;
        let path = self.entry.with_extension("lock");
        let started = Instant::now();

        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(ProcessCacheLock { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if started.elapsed() >= PROCESS_CACHE_LOCK_TIMEOUT {
                        bail!(
                            "timed out waiting for process cache entry {}; another process may still be building it",
                            self.entry.display()
                        );
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to lock process cache entry {}", self.entry.display())
                    });
                }
            }
        }
    }

    pub fn display(&self) -> &Path {
        &self.entry
    }

    fn at(root: &Path, identity: &str, variant: &str) -> Self {
        Self { entry: root.join(cache_key(identity)).join(cache_key(variant)) }
    }
}

impl Drop for ProcessCacheLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn cache_identity(input: &str) -> Result<String> {
    if input.starts_with("http://") || input.starts_with("https://") {
        return Ok(input.trim().to_string());
    }

    let path = fs::canonicalize(input)
        .with_context(|| format!("failed to canonicalize local process input {input}"))?;
    let metadata = fs::metadata(&path).with_context(|| {
        format!("failed to read local process input metadata {}", path.display())
    })?;
    let modified = metadata
        .modified()
        .with_context(|| {
            format!("failed to read local process input modification time {}", path.display())
        })?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(format!("{};bytes={};modified={modified}", path.display(), metadata.len()))
}

fn cache_key(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn is_complete_cache_entry(entry: &Path) -> Result<bool> {
    if !entry.is_dir() {
        return Ok(false);
    }
    let pack = entry.join("video.frameflame");
    Ok(find_video(entry)?.is_some()
        && pack.join("manifest.json").is_file()
        && pack.join("frames.jsonl").is_file()
        && has_file_with_extension(&pack.join("selected"), "jpg")?
        && has_file_with_extension(&entry.join("segments"), "mp4")?)
}

fn has_file_with_extension(dir: &Path, extension: &str) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some(extension) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn find_video(dir: &Path) -> Result<Option<PathBuf>> {
    let mut videos = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file() && path.file_stem().is_some_and(|stem| stem == "video"))
        .collect::<Vec<_>>();
    videos.sort();
    Ok(videos.into_iter().next())
}

fn clear_process_artifacts(destination: &Path) -> Result<()> {
    if !destination.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(destination)
        .with_context(|| format!("failed to read {}", destination.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let generated = matches!(name.to_str(), Some("segments" | "video.frameflame"))
            || name.to_string_lossy().starts_with("video.");
        if generated && path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove stale artifact {}", path.display()))?;
        } else if generated && path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale artifact {}", path.display()))?;
        }
    }
    Ok(())
}

fn copy_process_artifacts(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let name = entry.file_name();
        let destination_path = destination.join(&name);
        if source_path.is_dir() && matches!(name.to_str(), Some("segments" | "video.frameflame")) {
            copy_dir_contents(&source_path, &destination_path)?;
        } else if source_path.is_file() && name.to_string_lossy().starts_with("video.") {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn copy_dir_contents(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_contents(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

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
        sync::mpsc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
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

    #[test]
    fn local_process_cache_key_changes_when_the_source_changes() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-process-cache-test-{unique}"));
        fs::create_dir_all(&root)?;
        let video = root.join("recording.mp4");
        fs::write(&video, "first")?;
        let first = ProcessCache::for_input(video.to_str().unwrap_or_default(), "fast")?;
        fs::write(&video, "a different recording")?;
        let second = ProcessCache::for_input(video.to_str().unwrap_or_default(), "fast")?;

        assert_ne!(first.entry, second.entry);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn process_cache_restores_a_complete_evidence_pack() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-process-cache-test-{unique}"));
        let source = root.join("source");
        let restored = root.join("restored");
        let cache = ProcessCache::at(&root.join("cache"), "https://example.com/video", "fast");
        fs::create_dir_all(source.join("video.frameflame/selected"))?;
        fs::create_dir_all(source.join("segments"))?;
        fs::write(source.join("video.mp4"), "video")?;
        fs::write(source.join("video.frameflame/manifest.json"), "{}")?;
        fs::write(source.join("video.frameflame/frames.jsonl"), "{}")?;
        fs::write(source.join("video.frameflame/selected/000000.jpg"), "frame")?;
        fs::write(source.join("segments/000.mp4"), "segment")?;

        cache.store(&source)?;
        fs::create_dir_all(restored.join("segments"))?;
        fs::write(restored.join("segments/stale.mp4"), "stale")?;
        fs::write(restored.join("video.en.srt"), "stale captions")?;

        assert!(cache.restore(&restored)?);
        assert_eq!(fs::read_to_string(restored.join("video.mp4"))?, "video");
        assert!(restored.join("video.frameflame/selected/000000.jpg").is_file());
        assert!(!restored.join("segments/stale.mp4").exists());
        assert!(!restored.join("video.en.srt").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn process_cache_lock_waits_for_the_first_builder() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-process-cache-test-{unique}"));
        let cache = ProcessCache::at(&root.join("cache"), "https://example.com/video", "fast");
        let first = cache.lock()?;
        let second = ProcessCache::at(&root.join("cache"), "https://example.com/video", "fast");
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || sender.send(second.lock().is_ok()));

        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        assert!(receiver.recv_timeout(Duration::from_secs(1))?);
        worker.join().map_err(|_| anyhow::anyhow!("cache lock worker panicked"))??;
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn process_cache_rejects_incomplete_artifacts() -> Result<()> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let root = env::temp_dir().join(format!("flameframe-process-cache-test-{unique}"));
        let cache = ProcessCache::at(&root.join("cache"), "https://example.com/video", "fast");
        fs::create_dir_all(cache.entry.join("video.frameflame/selected"))?;
        fs::create_dir_all(cache.entry.join("segments"))?;
        fs::write(cache.entry.join("video.mp4"), "video")?;
        fs::write(cache.entry.join("video.frameflame/manifest.json"), "{}")?;
        fs::write(cache.entry.join("video.frameflame/frames.jsonl"), "{}")?;
        fs::write(cache.entry.join("video.frameflame/selected/not-a-frame.txt"), "not a frame")?;
        fs::write(cache.entry.join("segments/not-a-segment.txt"), "not a segment")?;

        assert!(!cache.restore(&root.join("restored"))?);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
