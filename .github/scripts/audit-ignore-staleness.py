#!/usr/bin/env python3
"""Warn when a suppressed advisory's crate has left the dependency graph.

`.cargo/audit.toml` (cargo-audit) and `osv-scanner.toml` (OpenSSF Scorecard's
vulnerability check) both carry a hand-justified ignore list for advisories
with no upstream fix. Nothing expires those entries, so an ignore silently
outlives its justification once the offending crate leaves the tree.

This check resolves each ignored advisory to its crate via the local RustSec
advisory database and asks `cargo tree -i <crate>` whether the crate is still
present. If it is gone, *neither* scanner can match the advisory, so the ignore
is definitively stale and should be deleted. We deliberately key on crate
presence rather than on cargo-audit's own match set: cargo-audit and osv-scanner
use different version matchers and disagree on some advisories (e.g. the
hickory-proto and memmap2 entries), so a "no longer matched by cargo-audit"
signal would raise false alarms. Crate presence is scanner-agnostic.

Non-blocking: emits GitHub `::warning::` annotations and always exits 0.
"""
from __future__ import annotations

import os
import sys
import tomllib
from pathlib import Path

DAEMON = Path(__file__).resolve().parents[2] / "source" / "daemon"
AUDIT_TOML = DAEMON / ".cargo" / "audit.toml"
# osv-scanner resolves its config per lockfile directory, so the fuzz
# workspace carries its own copy of the ignores relevant to fuzz/Cargo.lock.
OSV_TOMLS = [
    (DAEMON / "osv-scanner.toml", "osv-scanner.toml"),
    (DAEMON / "fuzz" / "osv-scanner.toml", "fuzz/osv-scanner.toml"),
]
# Every Cargo.lock osv-scanner walks; a crate lingering in any of them keeps
# the advisory reachable, so union them for the presence check.
LOCKFILES = [DAEMON / "Cargo.lock", DAEMON / "fuzz" / "Cargo.lock"]


def ignored_ids() -> dict[str, list[str]]:
    """Map advisory id -> list of config files that ignore it."""
    ids: dict[str, list[str]] = {}
    if AUDIT_TOML.exists():
        cfg = tomllib.loads(AUDIT_TOML.read_text())
        for adv in cfg.get("advisories", {}).get("ignore", []):
            ids.setdefault(adv, []).append(".cargo/audit.toml")
    for path, label in OSV_TOMLS:
        if path.exists():
            cfg = tomllib.loads(path.read_text())
            for entry in cfg.get("IgnoredVulns", []):
                if "id" in entry:
                    ids.setdefault(entry["id"], []).append(label)
    return ids


def advisory_db() -> Path:
    cargo_home = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    return Path(cargo_home) / "advisory-db"


def crate_for(adv_id: str, db: Path) -> str | None:
    matches = list(db.glob(f"crates/*/{adv_id}.md"))
    return matches[0].parent.name if matches else None


def locked_crates() -> set[str]:
    """Every package name across the scanned lockfiles.

    Keying on presence-in-lockfile mirrors exactly what cargo-audit and
    osv-scanner read (the whole Cargo.lock, all dependency kinds, all
    versions), and sidesteps `cargo tree -i` ambiguity when a crate appears at
    multiple versions. It is intentionally version-agnostic: it flags an ignore
    stale only once the crate is *entirely gone*, never on a version-scoped
    fix — the safe direction, since re-deriving affected ranges is the very
    work the scanners already do.
    """
    names: set[str] = set()
    for lock in LOCKFILES:
        if not lock.exists():
            continue
        cfg = tomllib.loads(lock.read_text())
        names.update(pkg["name"] for pkg in cfg.get("package", []))
    return names


def warn(msg: str) -> None:
    print(f"::warning::{msg}")


def main() -> int:
    db = advisory_db()
    if not db.exists():
        warn(f"RustSec advisory-db not found at {db}; skipping staleness check")
        return 0

    ids = ignored_ids()
    if not ids:
        print("No ignored advisories configured.")
        return 0

    present = locked_crates()
    stale, unknown, active = [], [], []
    for adv_id, files in sorted(ids.items()):
        crate = crate_for(adv_id, db)
        if crate is None:
            unknown.append(adv_id)
            continue
        if crate in present:
            active.append((adv_id, crate))
        else:
            stale.append((adv_id, crate, files))

    for adv_id, crate, files in stale:
        warn(
            f"{adv_id}: ignored crate '{crate}' is no longer in the dependency "
            f"graph — remove the ignore from {', '.join(sorted(set(files)))}."
        )
    for adv_id in unknown:
        warn(
            f"{adv_id}: could not resolve a crate in the advisory-db; verify the "
            f"id is still current (withdrawn/renamed advisories can be dropped)."
        )

    print(f"\nSummary: {len(active)} active, {len(stale)} stale, {len(unknown)} unresolved.")
    for adv_id, crate in active:
        print(f"  active  {adv_id} -> {crate} (still present)")
    for adv_id, crate, _ in stale:
        print(f"  STALE   {adv_id} -> {crate} (gone)")
    for adv_id in unknown:
        print(f"  ??      {adv_id} (unresolved)")

    # Non-blocking by design.
    return 0


if __name__ == "__main__":
    sys.exit(main())
