//! search.rs — Search a remote tree by name (E8-S7).
//!
//! Two strategies, one result shape:
//!
//! 1. **`find` over [`crate::exec`]** — one round-trip, the whole tree walked
//!    server-side. On a link with any latency this is the difference between a
//!    second and a minute, because a client-side walk pays an RTT per directory.
//! 2. **A bounded SFTP walk** — used when the server refuses `exec` (sftp-only
//!    chroots, `ForceCommand`, shell-less accounts) or has no `find`. Correct
//!    everywhere, but it is the slow path and so it is explicitly *bounded*.
//!
//! Which one ran is reported in [`SearchOutcome::strategy`]: a search that
//! quietly took twenty times longer should be visible, not a mystery.
//!
//! ## Results carry paths, not metadata
//!
//! Both strategies return only a path and a name. `find` cannot portably report
//! an entry's type — `-printf` is GNU-only and absent on BSD/macOS/busybox
//! servers — and a `stat` per hit would undo the single-round-trip advantage
//! that is the entire point of the exec path. The walker *could* return more,
//! but then results would silently change shape depending on which strategy ran,
//! which is worse than both being minimal. Jumping the pane to a hit's directory
//! (what the UI does with these) needs the path and nothing else.
//!
//! ## Nothing is ever silently truncated
//!
//! Every bound — result cap, depth cap, entry cap, the exec timeout — sets
//! [`SearchOutcome::truncated`], so the UI can say "showing the first N" rather
//! than presenting a partial answer as a complete one.

use tokio_util::sync::CancellationToken;

use crate::error::{EngineError, Result};
use crate::exec::{shell_quote, DEFAULT_EXEC_TIMEOUT};
use crate::fs::{EntryKind, RemoteFs};
use crate::session::Session;

/// One matching entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// Absolute remote path of the match.
    pub path: String,
    /// The match's base name (the part the query matched).
    pub name: String,
}

/// Which strategy produced a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    /// One server-side `find` round-trip.
    Exec,
    /// A client-side bounded SFTP walk.
    Walk,
}

/// Bounds applied to a search.
#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    /// Stop after this many matches.
    pub max_results: usize,
    /// How many directory levels below the root the walk may descend.
    pub max_depth: usize,
    /// How many entries the walk may examine in total.
    pub max_entries: usize,
}

impl Default for SearchOptions {
    /// Bounds sized for an interactive search rather than an exhaustive audit.
    ///
    /// Returns: 500 results, 12 levels, 50 000 entries examined. The entry cap is
    /// the one that matters: it is what stops a walk of `/` on a large server
    /// from running for minutes against a pane the user has already navigated
    /// away from.
    fn default() -> Self {
        Self {
            max_results: 500,
            max_depth: 12,
            max_entries: 50_000,
        }
    }
}

/// The result of a search.
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    /// The matches, in discovery order.
    pub hits: Vec<SearchHit>,
    /// Which strategy produced them.
    pub strategy: SearchStrategy,
    /// Whether a bound stopped the search before the tree was exhausted.
    pub truncated: bool,
}

/// Escape a query so it is matched literally by a shell glob.
///
/// `find -iname` takes a **glob**, not a substring, so the query is wrapped in
/// `*…*` to get substring behaviour. Any glob metacharacter the user typed must
/// therefore be neutralised first, or searching for `a*b` would silently match
/// far more than the user asked for. (This is separate from
/// [`shell_quote`], which stops the *shell* from interpreting the word; both are
/// needed, in this order.)
///
/// Arguments: `query` — the raw user query.
/// Returns: the query with `*`, `?`, `[`, `]`, and `\` backslash-escaped.
fn escape_glob(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for ch in query.chars() {
        if matches!(ch, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Build the remote `find` command line for a search.
///
/// Arguments: `root` — the absolute directory to search under; `query` — the raw
/// user query.
/// Returns: a command line whose root and pattern are both single-quoted, with
/// stderr discarded so unreadable subdirectories produce no noise.
fn find_command(root: &str, query: &str) -> String {
    format!(
        "find {} -iname {} 2>/dev/null",
        shell_quote(root),
        shell_quote(&format!("*{}*", escape_glob(query)))
    )
}

/// Split an absolute path into its base name.
///
/// Arguments: `path` — an absolute remote path.
/// Returns: the portion after the last `/`, or the whole path when it has none.
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Parse `find`'s newline-separated output into hits.
///
/// Arguments: `stdout` — raw bytes from the command; `root` — the search root,
/// which `find` echoes back as a match when it matches the query itself;
/// `limit` — the result cap.
/// Returns: the hits and whether `limit` cut the list short.
fn parse_find_output(stdout: &[u8], root: &str, limit: usize) -> (Vec<SearchHit>, bool) {
    let mut hits = Vec::new();
    let mut truncated = false;
    for line in String::from_utf8_lossy(stdout).lines() {
        let path = line.trim_end_matches('\r');
        // Skip blanks and the root itself: "the folder you are searching in"
        // is never a useful result.
        if path.is_empty() || path == root {
            continue;
        }
        if hits.len() >= limit {
            truncated = true;
            break;
        }
        hits.push(SearchHit {
            path: path.to_string(),
            name: base_name(path).to_string(),
        });
    }
    (hits, truncated)
}

/// Search using one server-side `find`.
///
/// Arguments: `session` — the session to run on; `root`/`query` — what to search;
/// `options` — bounds; `cancel` — abandons the round-trip.
/// Returns: `Ok(Some(outcome))` when `find` produced usable output, `Ok(None)`
/// when the server has no usable `find` and the caller should fall back, or
/// [`EngineError::Canceled`].
///
/// A **non-zero exit with output is still a usable result**, and this is
/// deliberate: `find` exits non-zero if any subdirectory was unreadable, which
/// on a real server (searching `/` as a non-root user) is the normal case, not
/// an error. Treating that as failure would send every such search down the slow
/// walk — which would hit exactly the same unreadable directories. Only a
/// completely empty non-zero result (`command not found`, refused exec) falls
/// back.
async fn search_via_exec(
    session: &Session,
    root: &str,
    query: &str,
    options: SearchOptions,
    cancel: &CancellationToken,
) -> Result<Option<SearchOutcome>> {
    let command = find_command(root, query);
    let output = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(EngineError::Canceled),
        result = session.exec(&command, DEFAULT_EXEC_TIMEOUT) => result,
    };

    let output = match output {
        Ok(output) => output,
        Err(EngineError::Canceled) => return Err(EngineError::Canceled),
        Err(e) => {
            tracing::debug!(error = %e, "find exec failed; falling back to an SFTP walk");
            return Ok(None);
        }
    };

    if !output.ok() && output.stdout.is_empty() {
        tracing::debug!(
            status = ?output.exit_status,
            "find unavailable; falling back to an SFTP walk"
        );
        return Ok(None);
    }

    let (hits, truncated) = parse_find_output(&output.stdout, root, options.max_results);
    Ok(Some(SearchOutcome {
        hits,
        strategy: SearchStrategy::Exec,
        truncated,
    }))
}

/// Search by walking the tree over SFTP.
///
/// The correctness baseline: it needs nothing but the SFTP subsystem. It costs a
/// round-trip per directory, hence the bounds in `options`.
///
/// Symlinks are matched by name but **never followed**, upholding the
/// project-wide rule — following them would both escape the search root and
/// allow a cycle to run the walk forever.
///
/// Arguments: `fs` — the filesystem to walk; `root`/`query` — what to search;
/// `options` — bounds; `cancel` — checked once per directory.
/// Returns: the outcome, or [`EngineError::Canceled`]. A directory that cannot
/// be listed (permissions) is skipped rather than failing the whole search.
async fn search_via_walk(
    fs: &dyn RemoteFs,
    root: &str,
    query: &str,
    options: SearchOptions,
    cancel: &CancellationToken,
) -> Result<SearchOutcome> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    let mut truncated = false;
    let mut examined = 0usize;
    // (path, depth); depth 0 is the root's own children.
    let mut stack = vec![(root.to_string(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(EngineError::Canceled);
        }
        let entries = match fs.list(&dir).await {
            Ok(entries) => entries,
            Err(e) => {
                // Unreadable subdirectories are expected, not fatal.
                tracing::debug!(dir = %dir, error = %e, "skipping unlistable directory");
                continue;
            }
        };
        for entry in entries {
            examined += 1;
            if examined > options.max_entries {
                truncated = true;
                break;
            }
            if entry.name.to_lowercase().contains(&needle) {
                if hits.len() >= options.max_results {
                    truncated = true;
                    break;
                }
                hits.push(SearchHit {
                    path: entry.path.clone(),
                    name: entry.name.clone(),
                });
            }
            if entry.kind == EntryKind::Dir && depth < options.max_depth {
                stack.push((entry.path, depth + 1));
            } else if entry.kind == EntryKind::Dir {
                truncated = true;
            }
        }
        if truncated && (hits.len() >= options.max_results || examined > options.max_entries) {
            break;
        }
    }

    Ok(SearchOutcome {
        hits,
        strategy: SearchStrategy::Walk,
        truncated,
    })
}

/// Search a remote tree for entries whose name contains `query`.
///
/// Prefers one server-side `find` and falls back to a bounded SFTP walk when the
/// server cannot run it. Matching is case-insensitive and by substring in both
/// strategies, so which one ran does not change *what* matches — only how fast
/// the answer arrives and how far it reaches.
///
/// Arguments: `session` — the session to search on; `root` — the absolute
/// directory to search under; `query` — the substring to look for; `options` —
/// bounds on the walk; `cancel` — cancels either strategy.
/// Returns: the matches with the strategy that found them, or
/// [`EngineError::Canceled`] if the token fired. An empty `query`, or a `root`
/// that is not absolute, is [`EngineError::InvalidPath`] — an unanchored root
/// would be interpreted by `find` relative to the login directory, which is not
/// the directory the user is looking at.
pub async fn search(
    session: &Session,
    root: &str,
    query: &str,
    options: SearchOptions,
    cancel: &CancellationToken,
) -> Result<SearchOutcome> {
    if query.trim().is_empty() {
        return Err(EngineError::InvalidPath("empty search query".into()));
    }
    if !root.starts_with('/') {
        return Err(EngineError::InvalidPath(format!(
            "search root must be absolute: {root}"
        )));
    }

    if let Some(outcome) = search_via_exec(session, root, query, options, cancel).await? {
        return Ok(outcome);
    }
    search_via_walk(&session.remote_fs().await, root, query, options, cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_metacharacters_in_the_query_are_matched_literally() {
        // Without escaping, `a*b` would match far more than the user typed.
        assert_eq!(escape_glob("a*b"), r"a\*b");
        assert_eq!(escape_glob("what?"), r"what\?");
        assert_eq!(escape_glob("[abc]"), r"\[abc\]");
        assert_eq!(escape_glob(r"back\slash"), r"back\\slash");
        assert_eq!(escape_glob("plain.txt"), "plain.txt");
    }

    #[test]
    fn find_command_quotes_both_root_and_pattern() {
        let cmd = find_command("/srv/app", "notes");
        assert_eq!(cmd, "find '/srv/app' -iname '*notes*' 2>/dev/null");
    }

    #[test]
    fn find_command_neutralizes_injection_in_the_root_and_the_query() {
        let cmd = find_command("/tmp/a'; rm -rf /; echo '", "x'; id; echo '");
        // Every quote the attacker supplied is escaped, so the command has
        // exactly three shell words before the redirect.
        assert!(cmd.starts_with("find '/tmp/a'\\''; rm -rf /; echo '\\''' -iname "));
        assert!(cmd.contains(r"'*x'\''; id; echo '\''*'"));
    }

    #[test]
    fn find_output_is_parsed_into_hits_without_the_root_itself() {
        let (hits, truncated) = parse_find_output(
            b"/srv/app\n/srv/app/notes.txt\n\n/srv/app/sub/notes.md\n",
            "/srv/app",
            10,
        );
        assert!(!truncated);
        assert_eq!(
            hits,
            vec![
                SearchHit {
                    path: "/srv/app/notes.txt".into(),
                    name: "notes.txt".into()
                },
                SearchHit {
                    path: "/srv/app/sub/notes.md".into(),
                    name: "notes.md".into()
                },
            ]
        );
    }

    #[test]
    fn hitting_the_result_cap_is_reported_not_hidden() {
        let (hits, truncated) = parse_find_output(b"/a/1\n/a/2\n/a/3\n", "/a", 2);
        assert_eq!(hits.len(), 2);
        assert!(truncated, "a capped result set must say so");
    }

    #[test]
    fn crlf_line_endings_do_not_leak_into_paths() {
        let (hits, _) = parse_find_output(b"/a/x.txt\r\n", "/a", 10);
        assert_eq!(hits[0].path, "/a/x.txt");
    }

    #[test]
    fn base_name_handles_roots_and_bare_names() {
        assert_eq!(base_name("/a/b/c.txt"), "c.txt");
        assert_eq!(base_name("c.txt"), "c.txt");
        assert_eq!(base_name("/"), "");
    }
}
