//! Three-tier ignore engine
//!
//! Tier 1 — System defaults: hidden files, `.DS_Store`, `.git/`, `node_modules/`,
//!           external symlinks, `.Trash/`
//! Tier 2 — Per-folder `.librarianignore` (gitignore syntax, scoped to that folder)
//! Tier 3 — Global `~/.librarian/ignore` (gitignore syntax, applied everywhere)

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Names that are always ignored regardless of any user configuration.
const SYSTEM_IGNORED_NAMES: &[&str] = &[".DS_Store", ".git", "node_modules", ".Trash"];

/// File name for per-directory ignore files.
const LOCAL_IGNORE_FILE: &str = ".librarianignore";

/// Three-tier ignore engine.
pub struct IgnoreEngine {
    /// Root of the scan; used for symlink resolution.
    source_dir: PathBuf,
    /// Global ignore matcher (Tier 3).  `None` when no global file exists.
    global: Option<Gitignore>,
    /// Per-directory matchers keyed by the **absolute** directory path (Tier 2).
    local: HashMap<PathBuf, Gitignore>,
}

impl IgnoreEngine {
    /// Build an ignore engine for the given source directory.
    ///
    /// Walks `source_dir` to discover every `.librarianignore` file and builds a
    /// matcher for each.  Optionally loads a global ignore file from
    /// `global_ignore_path`; when `None` the default `~/.librarian/ignore` is
    /// used if it exists.
    pub fn new(source_dir: &Path, global_ignore_path: Option<&Path>) -> anyhow::Result<Self> {
        let source_dir = source_dir
            .canonicalize()
            .unwrap_or_else(|_| source_dir.to_path_buf());

        // --- Tier 3: global ignore -----------------------------------------
        let resolved_global: Option<PathBuf> = match global_ignore_path {
            Some(p) => Some(p.to_path_buf()),
            None => default_global_ignore_path(),
        };

        let global = resolved_global
            .as_deref()
            .filter(|p| p.exists())
            .map(|p| {
                let parent = p.parent().unwrap_or(Path::new("/"));
                let mut builder = GitignoreBuilder::new(parent);
                builder.add(p);
                builder.build()
            })
            .transpose()?;

        // --- Tier 2: per-directory .librarianignore -------------------------
        let local = collect_local_ignores(&source_dir)?;

        Ok(Self {
            source_dir,
            global,
            local,
        })
    }

    /// Returns `true` when `path` should be excluded from processing.
    ///
    /// Evaluation order:
    /// 1. System defaults (Tier 1)
    /// 2. Per-folder `.librarianignore` (Tier 2) — innermost first
    /// 3. Global ignore (Tier 3)
    pub fn is_ignored(&self, path: &Path) -> bool {
        // Tier 1 ---------------------------------------------------------------
        if is_system_ignored(path) {
            return true;
        }

        let is_dir = path.is_dir();

        // Canonicalize so path prefixes align with the roots stored in local
        // matchers (which were also built from canonical paths).
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Tier 2 ---------------------------------------------------------------
        // Walk up from the file's immediate parent toward source_dir, checking
        // each directory that has a .librarianignore.  Inner (closer) rules
        // take precedence over outer ones.
        let canonical_parent = canonical.parent().unwrap_or(&canonical);
        let mut dir: &Path = canonical_parent;
        loop {
            if let Some(gi) = self.local.get(dir) {
                let m = gi.matched(&canonical, is_dir);
                if m.is_ignore() {
                    return true;
                }
                // An explicit negation (whitelist) short-circuits further
                // Tier-2 checks so the file is kept.
                if m.is_whitelist() {
                    break;
                }
            }
            match dir.parent() {
                Some(p) if dir != self.source_dir => dir = p,
                _ => break,
            }
        }

        // Tier 3 ---------------------------------------------------------------
        if let Some(gi) = &self.global
            && gi.matched(&canonical, is_dir).is_ignore()
        {
            return true;
        }

        false
    }

    /// Returns `true` when `path` is a symlink whose resolved target lies
    /// outside of `root`.
    pub fn is_external_symlink(path: &Path, root: &Path) -> bool {
        // Only act on symlinks; regular files/directories are never "external".
        if !path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return false;
        }

        let canonical_target = match fs::canonicalize(path) {
            Ok(p) => p,
            // Broken symlink — treat as external to be safe.
            Err(_) => return true,
        };

        let canonical_root = match fs::canonicalize(root) {
            Ok(p) => p,
            Err(_) => root.to_path_buf(),
        };

        !canonical_target.starts_with(&canonical_root)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the default global ignore path (`~/.librarian/ignore`), or `None`
/// when the home directory cannot be determined.
fn default_global_ignore_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join(".librarian").join("ignore"))
}

/// Returns `true` for paths that are always ignored (Tier 1).
fn is_system_ignored(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    // Hidden files (starting with `.`)
    if name.starts_with('.') {
        return true;
    }

    // Named constants: .DS_Store, .git, node_modules, .Trash
    // (Hidden-file rule already covers the dot-prefixed ones, but we check
    // explicitly so the list is self-documenting and covers future changes.)
    for &ignored in SYSTEM_IGNORED_NAMES {
        if name == ignored {
            return true;
        }
    }

    false
}

/// Walk `source_dir` recursively and build a `Gitignore` matcher for every
/// directory that contains a `.librarianignore` file.
fn collect_local_ignores(source_dir: &Path) -> anyhow::Result<HashMap<PathBuf, Gitignore>> {
    let mut map = HashMap::new();
    collect_local_ignores_recursive(source_dir, &mut map)?;
    Ok(map)
}

fn collect_local_ignores_recursive(
    dir: &Path,
    map: &mut HashMap<PathBuf, Gitignore>,
) -> anyhow::Result<()> {
    // Canonicalize so the root stored inside `Gitignore` matches the
    // canonicalized paths we will pass to `matched` later.
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());

    let ignore_file = canonical_dir.join(LOCAL_IGNORE_FILE);
    if ignore_file.exists() {
        let mut builder = GitignoreBuilder::new(&canonical_dir);
        builder.add(&ignore_file);
        let gi = builder.build()?;
        map.insert(canonical_dir.clone(), gi);
    }

    let entries = match fs::read_dir(&canonical_dir) {
        Ok(e) => e,
        // Permission error or other I/O problem — skip this directory.
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            // Respect system defaults even during collection.
            if !is_system_ignored(&path) {
                collect_local_ignores_recursive(&path, map)?;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;

    // Helper: create an IgnoreEngine rooted at `dir` with no global ignore.
    fn engine(dir: &Path) -> IgnoreEngine {
        IgnoreEngine::new(dir, Some(Path::new("/nonexistent/ignore"))).unwrap()
    }

    // -----------------------------------------------------------------------
    // Tier 1 — system defaults
    // -----------------------------------------------------------------------

    #[test]
    fn hidden_files_are_ignored() {
        let tmp = TempDir::new().unwrap();
        let hidden = tmp.path().join(".hidden_file");
        fs::write(&hidden, b"").unwrap();

        let eng = engine(tmp.path());
        assert!(eng.is_ignored(&hidden), ".hidden_file should be ignored");
    }

    #[test]
    fn ds_store_is_ignored() {
        let tmp = TempDir::new().unwrap();
        let ds = tmp.path().join(".DS_Store");
        fs::write(&ds, b"").unwrap();

        let eng = engine(tmp.path());
        assert!(eng.is_ignored(&ds), ".DS_Store should be ignored");
    }

    #[test]
    fn git_directory_is_ignored() {
        let tmp = TempDir::new().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        let eng = engine(tmp.path());
        assert!(eng.is_ignored(&git_dir), ".git/ should be ignored");
    }

    #[test]
    fn node_modules_is_ignored() {
        let tmp = TempDir::new().unwrap();
        let nm = tmp.path().join("node_modules");
        fs::create_dir(&nm).unwrap();

        let eng = engine(tmp.path());
        assert!(eng.is_ignored(&nm), "node_modules/ should be ignored");
    }

    // -----------------------------------------------------------------------
    // Normal files should NOT be ignored
    // -----------------------------------------------------------------------

    #[test]
    fn normal_files_not_ignored() {
        let tmp = TempDir::new().unwrap();
        let regular = tmp.path().join("document.pdf");
        fs::write(&regular, b"data").unwrap();

        let eng = engine(tmp.path());
        assert!(
            !eng.is_ignored(&regular),
            "regular files must not be ignored"
        );
    }

    // -----------------------------------------------------------------------
    // Tier 2 — per-folder .librarianignore
    // -----------------------------------------------------------------------

    #[test]
    fn local_librarianignore_respected() {
        let tmp = TempDir::new().unwrap();

        // Write a .librarianignore that excludes *.log files.
        let ignore_file = tmp.path().join(".librarianignore");
        fs::write(&ignore_file, b"*.log\n").unwrap();

        let log_file = tmp.path().join("server.log");
        fs::write(&log_file, b"").unwrap();

        let txt_file = tmp.path().join("notes.txt");
        fs::write(&txt_file, b"").unwrap();

        let eng = IgnoreEngine::new(tmp.path(), Some(Path::new("/nonexistent/ignore"))).unwrap();

        assert!(
            eng.is_ignored(&log_file),
            "*.log should be ignored by .librarianignore"
        );
        assert!(!eng.is_ignored(&txt_file), ".txt should not be ignored");
    }

    #[test]
    fn local_librarianignore_in_subdirectory() {
        let tmp = TempDir::new().unwrap();

        let sub = tmp.path().join("subdir");
        fs::create_dir(&sub).unwrap();

        // Only *.tmp ignored inside `subdir`.
        fs::write(sub.join(".librarianignore"), b"*.tmp\n").unwrap();

        let ignored = sub.join("cache.tmp");
        fs::write(&ignored, b"").unwrap();

        let not_ignored = tmp.path().join("cache.tmp");
        fs::write(&not_ignored, b"").unwrap();

        let eng = IgnoreEngine::new(tmp.path(), Some(Path::new("/nonexistent/ignore"))).unwrap();

        assert!(
            eng.is_ignored(&ignored),
            "file inside subdir should match subdir rule"
        );
        assert!(
            !eng.is_ignored(&not_ignored),
            "file outside subdir should not match subdir rule"
        );
    }

    // -----------------------------------------------------------------------
    // Tier 3 — global ignore
    // -----------------------------------------------------------------------

    #[test]
    fn global_ignore_respected() {
        let tmp = TempDir::new().unwrap();

        // Write a global ignore file that excludes *.bak.
        let global_file = tmp.path().join("global_ignore");
        fs::write(&global_file, b"*.bak\n").unwrap();

        let source = tmp.path().join("source");
        fs::create_dir(&source).unwrap();

        let bak_file = source.join("old.bak");
        fs::write(&bak_file, b"").unwrap();

        let txt_file = source.join("keep.txt");
        fs::write(&txt_file, b"").unwrap();

        let eng = IgnoreEngine::new(&source, Some(global_file.as_path())).unwrap();

        assert!(
            eng.is_ignored(&bak_file),
            "*.bak should be ignored by global ignore"
        );
        assert!(
            !eng.is_ignored(&txt_file),
            ".txt should not be ignored by global rule"
        );
    }

    // -----------------------------------------------------------------------
    // Symlinks
    // -----------------------------------------------------------------------

    #[test]
    fn external_symlinks_detected() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        fs::create_dir(&root).unwrap();

        // Create a target outside of `root`.
        let external_target = tmp.path().join("external.txt");
        fs::write(&external_target, b"outside").unwrap();

        let link = root.join("link_to_external");
        symlink(&external_target, &link).unwrap();

        assert!(
            IgnoreEngine::is_external_symlink(&link, &root),
            "symlink pointing outside root should be detected as external"
        );
    }

    #[test]
    fn internal_symlinks_not_flagged() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        fs::create_dir(&root).unwrap();

        let internal_target = root.join("real.txt");
        fs::write(&internal_target, b"inside").unwrap();

        let link = root.join("link_to_internal");
        symlink(&internal_target, &link).unwrap();

        assert!(
            !IgnoreEngine::is_external_symlink(&link, &root),
            "symlink pointing inside root must not be flagged as external"
        );
    }

    #[test]
    fn broken_symlink_treated_as_external() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("root");
        fs::create_dir(&root).unwrap();

        // Create a symlink pointing to a non-existent target.
        let link = root.join("broken_link");
        symlink(Path::new("/nonexistent_target_12345"), &link).unwrap();

        assert!(
            IgnoreEngine::is_external_symlink(&link, &root),
            "broken symlink should be treated as external"
        );
    }

    #[test]
    fn librarianignore_negation_whitelists_file() {
        let tmp = TempDir::new().unwrap();

        // Ignore all .log files, but whitelist important.log
        fs::write(
            tmp.path().join(".librarianignore"),
            "*.log\n!important.log\n",
        )
        .unwrap();

        let ignored_log = tmp.path().join("debug.log");
        fs::write(&ignored_log, b"").unwrap();

        let whitelisted = tmp.path().join("important.log");
        fs::write(&whitelisted, b"").unwrap();

        let eng = IgnoreEngine::new(tmp.path(), Some(Path::new("/nonexistent/ignore"))).unwrap();

        assert!(eng.is_ignored(&ignored_log), "debug.log should be ignored");
        assert!(
            !eng.is_ignored(&whitelisted),
            "important.log should be whitelisted by negation"
        );
    }

    #[test]
    fn regular_file_not_flagged_as_external_symlink() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("plain.txt");
        fs::write(&file, b"data").unwrap();

        assert!(
            !IgnoreEngine::is_external_symlink(&file, tmp.path()),
            "a regular file must not be flagged as an external symlink"
        );
    }
}
