//! Per-root ignore patterns.
//!
//! A root's `ignore_patterns` column holds a JSON array of patterns. A
//! path that matches is treated as if it were not on disk: the watcher
//! drops its events, the reconcile walk does not descend into it, and
//! the applier records it as gone if it was tracked before the pattern
//! was added.
//!
//! Pattern syntax, a small subset of gitignore:
//!
//! - A pattern without `/` is matched against every component of the
//!   path, so `node_modules` or `*.tmp` match at any depth.
//! - A pattern containing `/` is matched against the path relative to
//!   the root, starting at the root: `build/output`. A leading or
//!   trailing `/` is allowed and has no extra meaning.
//! - `*` matches any run of characters within one component, `?`
//!   matches one character, and a component that is exactly `**`
//!   matches any number of components, including none.
//! - Matching a directory ignores everything beneath it.
//!
//! Matching is case-sensitive.
//!
//! Separately from the patterns, fsPulse never observes its own files:
//! its database (and SQLite's side files) and its log directory. If one
//! of them is inside a root, every write fsPulse makes would be a change
//! to record, which is another write. They are excluded for every root,
//! by absolute path, and cannot be turned back on.

use std::path::{Component, Path, PathBuf};

use once_cell::sync::OnceCell;

use rusqlite::{Connection, OptionalExtension};

use crate::error::FsPulseError;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// Matches any single component of the path.
    Name(String),
    /// Matches a prefix of the path relative to the root.
    Anchored(Vec<String>),
}

/// The compiled ignore patterns of one root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoreSet {
    patterns: Vec<Pattern>,
    /// Absolute paths excluded along with everything beneath them:
    /// fsPulse's own files.
    excluded: Vec<PathBuf>,
}

static OWN_FILES: OnceCell<Vec<PathBuf>> = OnceCell::new();

/// Tell the observation code where fsPulse's own files are. Called once
/// at server startup with the database file and the log directory.
/// SQLite's side files next to the database are added here.
pub fn set_own_files(database_file: &Path, log_dir: &Path) {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let database_file = canonical(database_file);
    let mut own = vec![canonical(log_dir)];
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = database_file.clone().into_os_string();
        name.push(suffix);
        own.push(PathBuf::from(name));
    }
    let _ = OWN_FILES.set(own);
}

fn own_files() -> Vec<PathBuf> {
    OWN_FILES.get().cloned().unwrap_or_default()
}

impl IgnoreSet {
    /// Compile a list of patterns. Blank patterns are dropped.
    pub fn new<S: AsRef<str>>(patterns: &[S]) -> IgnoreSet {
        let patterns = patterns
            .iter()
            .filter_map(|raw| {
                let trimmed = raw.as_ref().trim();
                let components: Vec<String> = trimmed
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect();
                match components.len() {
                    0 => None,
                    1 if !trimmed.trim_end_matches('/').contains('/') => {
                        Some(Pattern::Name(components.into_iter().next()?))
                    }
                    _ => Some(Pattern::Anchored(components)),
                }
            })
            .collect();
        IgnoreSet {
            patterns,
            excluded: Vec::new(),
        }
    }

    /// Also exclude these absolute paths and everything beneath them.
    pub fn excluding(mut self, paths: Vec<PathBuf>) -> IgnoreSet {
        self.excluded = paths;
        self
    }

    /// Parse the value of `roots.ignore_patterns`. `None` or an empty
    /// string means no patterns.
    pub fn from_json(json: Option<&str>) -> Result<IgnoreSet, FsPulseError> {
        match json.map(str::trim) {
            None | Some("") => Ok(IgnoreSet::default()),
            Some(text) => {
                let patterns: Vec<String> = serde_json::from_str(text).map_err(|e| {
                    FsPulseError::Error(format!("Invalid ignore_patterns '{text}': {e}"))
                })?;
                Ok(IgnoreSet::new(&patterns))
            }
        }
    }

    /// Encode patterns for `roots.ignore_patterns`: trimmed, blanks and
    /// repeats removed, order kept. `None` (stored as NULL) when nothing
    /// is left.
    pub fn to_stored_json<S: AsRef<str>>(patterns: &[S]) -> Result<Option<String>, FsPulseError> {
        let mut cleaned: Vec<&str> = Vec::new();
        for pattern in patterns {
            let pattern = pattern.as_ref().trim();
            if !pattern.is_empty() && !cleaned.contains(&pattern) {
                cleaned.push(pattern);
            }
        }
        if cleaned.is_empty() {
            return Ok(None);
        }
        serde_json::to_string(&cleaned)
            .map(Some)
            .map_err(|e| FsPulseError::Error(format!("Could not encode ignore patterns: {e}")))
    }

    /// Parse the value of `roots.ignore_patterns` for use while
    /// observing. A value that cannot be parsed is logged and treated as
    /// no patterns: a bad setting must not stop changes being recorded.
    ///
    /// The result also excludes fsPulse's own files.
    pub fn from_json_lenient(json: Option<&str>, root_id: i64) -> IgnoreSet {
        IgnoreSet::from_json(json)
            .unwrap_or_else(|e| {
                log::warn!("Root {root_id}: {e}; ignoring nothing");
                IgnoreSet::default()
            })
            .excluding(own_files())
    }

    /// Load the patterns stored for a root, leniently. A root that does
    /// not exist has no patterns.
    pub fn load(conn: &Connection, root_id: i64) -> Result<IgnoreSet, FsPulseError> {
        let json: Option<Option<String>> = conn
            .query_row(
                "SELECT ignore_patterns FROM roots WHERE root_id = ?",
                [root_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(IgnoreSet::from_json_lenient(json.flatten().as_deref(), root_id))
    }

    /// True if `path`, or any directory above it up to the root, matches
    /// a pattern. `path` must be absolute and under `root_path`; a path
    /// outside the root, and the root itself, are never ignored.
    pub fn is_ignored(&self, root_path: &Path, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(root_path) else {
            return false;
        };
        if self.excluded.iter().any(|own| path.starts_with(own)) {
            return true;
        }
        if self.patterns.is_empty() {
            return false;
        }
        let components: Vec<&str> = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();
        if components.is_empty() {
            return false;
        }

        self.patterns.iter().any(|pattern| match pattern {
            Pattern::Name(glob) => components.iter().any(|name| glob_match(glob, name)),
            Pattern::Anchored(globs) => matches_prefix(globs, &components),
        })
    }
}

/// True if `globs` matches some prefix of `components` (so that matching
/// a directory also matches everything beneath it).
fn matches_prefix(globs: &[String], components: &[&str]) -> bool {
    match globs.split_first() {
        None => true,
        Some((glob, rest)) if glob == "**" => {
            // `**` consumes zero or more components.
            (0..=components.len()).any(|skip| matches_prefix(rest, &components[skip..]))
        }
        Some((glob, rest)) => match components.split_first() {
            Some((name, tail)) => glob_match(glob, name) && matches_prefix(rest, tail),
            None => false,
        },
    }
}

/// Match one path component against a glob with `*` and `?`.
fn glob_match(glob: &str, name: &str) -> bool {
    let glob: Vec<char> = glob.chars().collect();
    let name: Vec<char> = name.chars().collect();

    // Iterative wildcard match with backtracking to the last `*`.
    let (mut g, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if g < glob.len() && (glob[g] == '?' || glob[g] == name[n]) {
            g += 1;
            n += 1;
        } else if g < glob.len() && glob[g] == '*' {
            star = Some((g, n));
            g += 1;
        } else if let Some((star_g, star_n)) = star {
            g = star_g + 1;
            n = star_n + 1;
            star = Some((star_g, star_n + 1));
        } else {
            return false;
        }
    }
    glob[g..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignored(patterns: &[&str], path: &str) -> bool {
        IgnoreSet::new(patterns).is_ignored(Path::new("/root"), Path::new(path))
    }

    #[test]
    fn glob_matches_within_one_component() {
        assert!(glob_match("*.tmp", "a.tmp"));
        assert!(glob_match("*.tmp", ".tmp"));
        assert!(!glob_match("*.tmp", "a.tmpx"));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
        assert!(glob_match("?.txt", "a.txt"));
        assert!(!glob_match("?.txt", "ab.txt"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("", "x"));
        assert!(glob_match("ca*he", "cache"));
    }

    #[test]
    fn name_pattern_matches_at_any_depth_and_covers_the_subtree() {
        assert!(ignored(&["node_modules"], "/root/node_modules"));
        assert!(ignored(&["node_modules"], "/root/a/node_modules"));
        assert!(ignored(&["node_modules"], "/root/a/node_modules/pkg/index.js"));
        assert!(!ignored(&["node_modules"], "/root/a/node_modules_old/x"));
        assert!(ignored(&["*.tmp"], "/root/a/b/file.tmp"));
        assert!(!ignored(&["*.tmp"], "/root/a/b/file.txt"));
    }

    #[test]
    fn anchored_pattern_matches_from_the_root() {
        assert!(ignored(&["build/output"], "/root/build/output"));
        assert!(ignored(&["build/output"], "/root/build/output/x/y"));
        assert!(!ignored(&["build/output"], "/root/a/build/output"));
        assert!(!ignored(&["build/output"], "/root/build"));
        // A leading slash anchors a single name at the root.
        assert!(ignored(&["/cache"], "/root/cache/x"));
        assert!(!ignored(&["/cache"], "/root/a/cache/x"));
        // A trailing slash does not anchor.
        assert!(ignored(&["cache/"], "/root/a/cache/x"));
    }

    #[test]
    fn double_star_spans_directories() {
        assert!(ignored(&["a/**/z"], "/root/a/z"));
        assert!(ignored(&["a/**/z"], "/root/a/b/c/z/file"));
        assert!(!ignored(&["a/**/z"], "/root/b/a/z"));
        assert!(ignored(&["**/logs/*.log"], "/root/x/y/logs/app.log"));
        assert!(!ignored(&["**/logs/*.log"], "/root/x/y/logs/app.txt"));
    }

    #[test]
    fn root_and_outside_paths_are_never_ignored() {
        assert!(!ignored(&["*"], "/root"));
        assert!(!ignored(&["*"], "/elsewhere/file"));
        assert!(ignored(&["*"], "/root/file"));
    }

    #[test]
    fn own_files_are_excluded_by_absolute_path() {
        let set = IgnoreSet::default().excluding(vec![
            PathBuf::from("/root/data/fspulse.db"),
            PathBuf::from("/root/data/fspulse.db-wal"),
            PathBuf::from("/root/data/logs"),
        ]);
        let ignored = |path: &str| set.is_ignored(Path::new("/root"), Path::new(path));
        assert!(ignored("/root/data/fspulse.db"));
        assert!(ignored("/root/data/fspulse.db-wal"));
        assert!(ignored("/root/data/logs/fspulse_2026.log"));
        // Whole names only: a neighbour with a longer name is not ours.
        assert!(!ignored("/root/data/fspulse.db.backup"));
        assert!(!ignored("/root/data/other.txt"));
        // Outside the root nothing is ignored, ours or not.
        assert!(!set.is_ignored(Path::new("/elsewhere"), Path::new("/root/data/fspulse.db")));
    }

    #[test]
    fn json_parsing() {
        assert!(IgnoreSet::from_json(None).unwrap().patterns.is_empty());
        assert!(IgnoreSet::from_json(Some("")).unwrap().patterns.is_empty());
        assert!(IgnoreSet::from_json(Some("[]")).unwrap().patterns.is_empty());
        assert!(IgnoreSet::from_json(Some("[\"  \"]")).unwrap().patterns.is_empty());
        let set = IgnoreSet::from_json(Some(r#"[".git", "build/output"]"#)).unwrap();
        assert!(set.is_ignored(Path::new("/root"), Path::new("/root/.git/HEAD")));
        assert!(IgnoreSet::from_json(Some("not json")).is_err());
    }

    #[test]
    fn stored_form_is_cleaned() {
        assert_eq!(IgnoreSet::to_stored_json::<&str>(&[]).unwrap(), None);
        assert_eq!(IgnoreSet::to_stored_json(&["  ", ""]).unwrap(), None);
        assert_eq!(
            IgnoreSet::to_stored_json(&[" .git ", "*.tmp", ".git"]).unwrap(),
            Some(r#"[".git","*.tmp"]"#.to_string())
        );
    }
}
