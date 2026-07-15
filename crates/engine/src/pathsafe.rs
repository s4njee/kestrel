//! pathsafe.rs — Validate untrusted remote path components before recreating a
//! remote directory tree on the local filesystem.
//!
//! A malicious or buggy server can return entry names like `..`, `/etc/passwd`,
//! `a/b`, `CON`, or names with NUL/control characters. Writing those blindly
//! under a download root risks path traversal or clobbering system files. Every
//! component is validated by [`safe_component`]; [`safe_join`] validates each
//! component of a relative path and returns a path guaranteed to stay under the
//! destination root. See tasks.md "Conventions & invariants".

use std::path::{Path, PathBuf};

use crate::error::{EngineError, Result};

/// Windows device names that are illegal as file names (case-insensitive, with
/// or without an extension).
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a single path component (one directory or file name).
///
/// Arguments: `name` — the untrusted component.
/// Returns: `Ok(())` if the name is safe to use as a local file/dir name;
/// [`EngineError::InvalidPath`] otherwise. Rejects: empty, `.`/`..`, embedded
/// `/` or `\`, NUL/control chars, Windows-reserved device names, and trailing
/// dots or spaces (which Windows silently strips).
pub fn safe_component(name: &str) -> Result<()> {
    let reject = |why: &str| Err(EngineError::InvalidPath(format!("{why}: {name:?}")));

    if name.is_empty() {
        return reject("empty component");
    }
    if name == "." || name == ".." {
        return reject("relative component");
    }
    if name.contains('/') || name.contains('\\') {
        return reject("embedded separator");
    }
    if name.chars().any(|c| c == '\0' || c.is_control()) {
        return reject("control character");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return reject("trailing dot or space");
    }
    // Windows reserved names, ignoring any extension (e.g. "CON.txt").
    let stem = name.split('.').next().unwrap_or(name);
    if WINDOWS_RESERVED
        .iter()
        .any(|r| stem.eq_ignore_ascii_case(r))
    {
        return reject("reserved device name");
    }
    Ok(())
}

/// Join a POSIX-style relative path onto a local root, validating every
/// component and asserting the result stays under the root.
///
/// Arguments:
/// - `root`: the download destination directory.
/// - `rel`: a `/`-separated relative path built from remote entry names.
///
/// Returns: the safe joined path, or [`EngineError::InvalidPath`] if any
/// component is unsafe or the join would escape `root`.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for component in rel.split('/').filter(|c| !c.is_empty()) {
        safe_component(component)?;
        out.push(component);
    }
    // Belt-and-suspenders: with every component validated (no `..`, no
    // separators) the result cannot escape, but assert it lexically anyway.
    if !out.starts_with(root) {
        return Err(EngineError::InvalidPath(format!(
            "path escapes destination root: {rel:?}"
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ordinary names (including reserved-name lookalikes) are accepted.
    #[test]
    fn accepts_ordinary_names() {
        for name in ["file.txt", "My Document", "naïve", "a.tar.gz", "CONsole", "COM10"] {
            assert!(safe_component(name).is_ok(), "should accept {name:?}");
        }
    }

    /// Traversal (`..`), current-dir, empty, and separators are rejected.
    #[test]
    fn rejects_traversal_and_separators() {
        for name in ["..", ".", "", "a/b", "a\\b", "/etc", "../../etc/passwd"] {
            assert!(safe_component(name).is_err(), "should reject {name:?}");
        }
    }

    /// NUL and other control characters are rejected.
    #[test]
    fn rejects_control_chars_and_nul() {
        assert!(safe_component("a\0b").is_err());
        assert!(safe_component("tab\tname").is_err());
        assert!(safe_component("newline\nname").is_err());
    }

    /// Windows-reserved names and trailing space/dot names are rejected.
    #[test]
    fn rejects_windows_reserved_and_trailing() {
        for name in ["CON", "nul", "Aux", "COM1", "LPT9", "CON.txt", "name ", "name."] {
            assert!(safe_component(name).is_err(), "should reject {name:?}");
        }
    }

    /// A valid multi-segment relative path joins under the root.
    #[test]
    fn safe_join_builds_under_root() {
        let root = Path::new("/downloads");
        let joined = safe_join(root, "sub/dir/file.txt").unwrap();
        assert_eq!(joined, Path::new("/downloads/sub/dir/file.txt"));
        assert!(joined.starts_with(root));
    }

    /// Paths that escape the root (or contain a bad component) are rejected.
    #[test]
    fn safe_join_rejects_escaping_paths() {
        let root = Path::new("/downloads");
        assert!(safe_join(root, "../etc/passwd").is_err());
        assert!(safe_join(root, "sub/../../escape").is_err());
        assert!(safe_join(root, "ok/CON/bad").is_err());
        assert!(safe_join(root, "a\0b").is_err());
    }
}
