//! Idempotent writer for the polkit rule that authorises Safe
//! Reboot / Safe Shutdown for the `wardnet` user.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Context;

/// Filename of the rule under `rules.d/`. The numeric prefix orders
/// the rule in polkit's evaluation sequence; 50 puts it after most
/// distro defaults but before any operator override at 60+.
pub const RULE_FILENAME: &str = "50-wardnet-power.rules";

/// Default base directory on a real Pi. Tests pass tempdir paths.
pub const DEFAULT_RULES_DIR: &str = "/etc/polkit-1/rules.d";

/// Embedded rule body. The trailing newline matters: polkit's parser
/// is whitespace-tolerant but operators reading the file expect a
/// final newline like every other config file.
pub const RULE_BODY: &str = include_str!("polkit_power.rules.in");

/// File mode applied to the rule file. Polkit reads it as root, so
/// world-readable is fine and matches every other rule under
/// `rules.d/`.
const RULE_MODE: u32 = 0o644;

/// Write the rule to `<base>/50-wardnet-power.rules`.
///
/// Idempotent: if a file with identical content already exists this
/// returns `Ok(())` without rewriting. Otherwise it writes a sibling
/// temp file, fsyncs it, renames into place, and fsyncs the parent
/// directory so a power loss after the rename leaves the new content
/// (rather than zeroes) on disk.
///
/// Polkitd `inotify`-watches `rules.d/` so no reload step is needed.
pub fn write_rule(base: &Path) -> anyhow::Result<()> {
    let target = base.join(RULE_FILENAME);

    // Idempotent fast path: bytes already match. Don't touch mtime.
    if let Ok(existing) = fs::read(&target)
        && existing == RULE_BODY.as_bytes()
    {
        // Even on a no-op pass, make sure the mode is what we want.
        // Operators sometimes hand-chmod files; reconciling here
        // means a re-run after such a tweak puts mode back to 0644.
        ensure_mode(&target)?;
        return Ok(());
    }

    fs::create_dir_all(base)
        .with_context(|| format!("creating polkit rules dir {}", base.display()))?;

    // Sibling temp file so the rename is atomic on the same fs.
    // Leading '.' keeps the partial file out of polkit's inotify
    // attention until we rename it onto the final name.
    let tmp = base.join(format!(".{RULE_FILENAME}.tmp"));

    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("creating temp file {}", tmp.display()))?;
        file.write_all(RULE_BODY.as_bytes())
            .with_context(|| format!("writing temp file {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("fsync temp file {}", tmp.display()))?;
        // Explicitly set permissions before the rename so polkitd
        // never sees the file with default umask permissions.
        fs::set_permissions(&tmp, fs::Permissions::from_mode(RULE_MODE))
            .with_context(|| format!("chmod temp file {}", tmp.display()))?;
    }

    fs::rename(&tmp, &target)
        .with_context(|| format!("rename {} -> {}", tmp.display(), target.display()))?;

    // Fsync the parent so the rename hits disk before we return —
    // otherwise a sudden power loss could replay an old metadata
    // state where the rename hasn't been persisted.
    fsync_dir(base)?;

    Ok(())
}

/// Ensure the existing file is mode 0644. Returns Ok if already so.
fn ensure_mode(path: &Path) -> anyhow::Result<()> {
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let current = meta.permissions().mode() & 0o777;
    if current != RULE_MODE {
        fs::set_permissions(path, fs::Permissions::from_mode(RULE_MODE))
            .with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

/// Open the directory and call `fsync(2)` on it.
fn fsync_dir(dir: &Path) -> anyhow::Result<()> {
    let f = fs::File::open(dir).with_context(|| format!("open dir {}", dir.display()))?;
    f.sync_all()
        .with_context(|| format!("fsync dir {}", dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn writes_expected_content_and_mode() {
        let dir = TempDir::new().unwrap();
        write_rule(dir.path()).unwrap();

        let file = dir.path().join(RULE_FILENAME);
        let bytes = fs::read(&file).unwrap();
        assert_eq!(bytes, RULE_BODY.as_bytes());

        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "expected 0644, got {mode:o}");
    }

    #[test]
    fn rule_body_authorises_wardnet_user_for_login1_actions() {
        // Spot-check the embedded rule body — guards against an
        // accidental edit that no longer matches the four action ids
        // the daemon needs.
        assert!(RULE_BODY.contains("subject.user !== \"wardnet\""));
        assert!(RULE_BODY.contains("org.freedesktop.login1.reboot"));
        assert!(RULE_BODY.contains("org.freedesktop.login1.reboot-multiple-sessions"));
        assert!(RULE_BODY.contains("org.freedesktop.login1.power-off"));
        assert!(RULE_BODY.contains("org.freedesktop.login1.power-off-multiple-sessions"));
        assert!(RULE_BODY.contains("polkit.Result.YES"));
    }

    #[test]
    fn idempotent_rerun_preserves_mtime() {
        let dir = TempDir::new().unwrap();
        write_rule(dir.path()).unwrap();
        let file = dir.path().join(RULE_FILENAME);
        let mtime_before = fs::metadata(&file).unwrap().modified().unwrap();

        // Sleep briefly so a rewrite would produce a different mtime
        // even on filesystems with second-resolution timestamps.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        write_rule(dir.path()).unwrap();
        let mtime_after = fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "second run rewrote the file even though content matched"
        );
    }

    #[test]
    fn mismatched_content_is_rewritten() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join(RULE_FILENAME);
        fs::write(&file, b"// stale content from a hand edit\n").unwrap();

        write_rule(dir.path()).unwrap();
        let bytes = fs::read(&file).unwrap();
        assert_eq!(bytes, RULE_BODY.as_bytes());
    }

    #[test]
    fn idempotent_rerun_restores_mode_if_chmodded() {
        let dir = TempDir::new().unwrap();
        write_rule(dir.path()).unwrap();
        let file = dir.path().join(RULE_FILENAME);

        // Operator chmods to 0600 by mistake; re-running the
        // migration should put it back to 0644 without rewriting
        // the content.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        write_rule(dir.path()).unwrap();

        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }

    #[test]
    fn creates_base_directory_if_missing() {
        let parent = TempDir::new().unwrap();
        let base = parent.path().join("not-yet-created");
        write_rule(&base).unwrap();

        assert!(base.join(RULE_FILENAME).exists());
    }
}
