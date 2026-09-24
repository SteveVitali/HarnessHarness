//! The Stage-1 path-pattern matcher (interim — same discipline as
//! `hh_monitor::args::scope_covers` under ADR-0212/OQ-132: the closed
//! `ResourcePattern` grammar is still open, so the containment path rules use
//! the declared interim: **normalised component-wise** matching).
//!
//! Rules (one scheme — CC1; used by [`crate::admit`] and the EP2 model gate):
//!
//! - `normalize_path` — strip a leading `./`, collapse repeated `/`, drop a
//!   trailing `/`. `..` segments are preserved as components (a path that
//!   names `..` is *not* rewritten here — the gate denies anything whose
//!   resolved spelling escapes a writable root, which the component checks
//!   enforce).
//! - `within(root, path)` — `path` equals `root` or sits under it
//!   (`root/` prefix at a component boundary).
//! - `pattern_matches(pattern, path)` — a `/`-leading pattern is an
//!   *anchored* prefix match; any other pattern is a **component-suffix**
//!   match (`.git/hooks` matches `work/repo/.git/hooks/x` at any depth inside
//!   a writable root — I-C2's "inside every `WritableRoot`").

/// Normalise a path spelling (component-wise; `..` kept as a component so an
/// escape attempt is *denied by the checks*, not rewritten away).
pub fn normalize_path(p: &str) -> String {
    let mut s = p.trim();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest;
    }
    let mut out = String::with_capacity(s.len());
    let mut last_slash = false;
    for c in s.chars() {
        if c == '/' {
            if last_slash {
                continue;
            }
            last_slash = true;
        } else {
            last_slash = false;
        }
        out.push(c);
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

fn components(s: &str) -> Vec<&str> {
    s.split('/').filter(|c| !c.is_empty()).collect()
}

/// `path` equals `root` or sits under it (component boundary — `work` does
/// not cover `workshop`). A `..` component anywhere in `path` (or `root`)
/// is *not within* — the matcher never rewrites an escape attempt; the gate
/// denies it instead (fail-closed on any spelled traversal).
pub fn within(root: &str, path: &str) -> bool {
    let root = normalize_path(root);
    let path = normalize_path(path);
    let rc = components(&root);
    let pc = components(&path);
    if rc.is_empty() || pc.is_empty() {
        return false;
    }
    if rc.iter().chain(pc.iter()).any(|c| *c == "..") {
        return false;
    }
    pc.len() >= rc.len() && pc[..rc.len()] == rc[..]
}

/// `pattern` matches `path`:
///
/// - a `/`-leading pattern is an anchored component-prefix match;
/// - any other pattern matches when the pattern's components appear as a
///   **contiguous window** in the path's components — the protected/denied
///   name itself *or any descendant* at any depth (a protected name like
///   `.git/hooks` denies `repo/.git/hooks/x` inside a writable root —
///   I-C2's "inside every `WritableRoot`").
pub fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize_path(pattern);
    let path = normalize_path(path);
    if pattern.is_empty() || path.is_empty() {
        return false;
    }
    let pc = components(&pattern);
    let tc = components(&path);
    if pattern.starts_with('/') {
        // Anchored: the pattern's components are a prefix of the path's.
        let pc = components(pattern.trim_start_matches('/'));
        if pc.len() > tc.len() {
            return false;
        }
        return pc.iter().zip(tc.iter()).all(|(p, t)| p == t);
    }
    if pc.len() > tc.len() {
        return false;
    }
    tc.windows(pc.len()).any(|w| *w == pc[..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn within_is_component_bounded() {
        assert!(within("workspace", "workspace"));
        assert!(within("workspace", "workspace/notes.txt"));
        assert!(within("workspace/", "workspace/a/b"));
        assert!(!within("workspace", "workshop/x"));
        assert!(!within("workspace", "other/workspace"));
        assert!(!within("workspace", "workspace/../escape"));
    }

    #[test]
    fn suffix_matching_holds_at_any_depth() {
        assert!(pattern_matches(
            ".git/hooks",
            "work/repo/.git/hooks/post-commit"
        ));
        assert!(pattern_matches(".git/hooks", ".git/hooks"));
        assert!(!pattern_matches(".git/hooks", "work/.git/hooks.d/x"));
        assert!(pattern_matches("/etc/ssh", "/etc/ssh/config"));
        assert!(!pattern_matches("/etc/ssh", "work/etc/ssh"));
    }
}
