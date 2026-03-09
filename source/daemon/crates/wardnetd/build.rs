//! Build script for `wardnetd`.
//!
//! 1. Derives a SemVer-compliant version string from `git describe` and exposes
//!    it as the `WARDNET_VERSION` compile-time environment variable.
//! 2. Parses the IEEE MA-L (OUI) CSV database committed at `data/oui.csv` and
//!    generates a Rust source file with a static array of all OUI-to-manufacturer
//!    mappings, included at compile time by `src/oui.rs` via `include!`.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;

// Pull in the shared version-parsing helpers so the logic is defined once.
include!("../../build-support/version.rs");

fn main() {
    // Rerun when the git HEAD or any ref changes.
    println!("cargo:rerun-if-changed=../../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../.git/refs/");

    let version = git_version().unwrap_or_else(cargo_pkg_version);
    println!("cargo:rustc-env=WARDNET_VERSION={version}");

    // Generate the OUI lookup table from the IEEE CSV database.
    generate_oui_data();
}

/// Attempt to derive a version string from `git describe`.
///
/// Returns `None` if git is unavailable or the command fails.
fn git_version() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let describe = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if describe.is_empty() {
        return None;
    }

    Some(parse_git_describe(&describe))
}

// ---------------------------------------------------------------------------
// OUI database code generation
// ---------------------------------------------------------------------------

/// Parse the IEEE MA-L OUI CSV and write a generated Rust source file to
/// `$OUT_DIR/oui_data.rs` containing a static array of `([u8; 3], &str)` entries.
fn generate_oui_data() {
    println!("cargo::rerun-if-changed=data/oui.csv");

    let csv_path = Path::new("data/oui.csv");
    assert!(
        csv_path.exists(),
        "OUI database not found at data/oui.csv. \
         Download it from https://standards-oui.ieee.org/oui/oui.csv"
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("oui_data.rs");

    let file = fs::File::open(csv_path).expect("failed to open oui.csv");
    let reader = BufReader::new(file);

    let mut entries = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line.expect("failed to read line");

        // Skip the header row.
        if i == 0 {
            continue;
        }

        // CSV columns: Registry, Assignment, Organization Name, Organization Address.
        // We need fields[1] (Assignment: 6-char hex OUI) and fields[2] (Org Name).
        let fields = parse_csv_line(&line);
        if fields.len() < 3 {
            continue;
        }

        let assignment = &fields[1];
        let org_name = &fields[2];

        if assignment.len() != 6 {
            continue;
        }

        let b0 = u8::from_str_radix(&assignment[0..2], 16);
        let b1 = u8::from_str_radix(&assignment[2..4], 16);
        let b2 = u8::from_str_radix(&assignment[4..6], 16);

        if let (Ok(b0), Ok(b1), Ok(b2)) = (b0, b1, b2) {
            // Sanitize: strip invisible/control characters, then escape for Rust.
            let sanitized = sanitize_org_name(org_name);
            if sanitized.is_empty() {
                continue;
            }
            let escaped = sanitized.replace('\\', "\\\\").replace('"', "\\\"");
            entries.push(format!(
                "([0x{b0:02X}, 0x{b1:02X}, 0x{b2:02X}], \"{escaped}\")"
            ));
        }
    }

    let mut out = fs::File::create(&out_path).expect("failed to create oui_data.rs");
    // Suppress clippy warnings for non-NFC Unicode in vendor names from the IEEE database.
    writeln!(out, "#[allow(clippy::unicode_not_nfc)]").unwrap();
    writeln!(out, "/// Auto-generated from the IEEE MA-L OUI database.").unwrap();
    writeln!(out, "/// Contains {} entries.", entries.len()).unwrap();
    writeln!(out, "static OUI_ENTRIES: &[([u8; 3], &str)] = &[").unwrap();
    for entry in &entries {
        writeln!(out, "    {entry},").unwrap();
    }
    writeln!(out, "];").unwrap();

    eprintln!("oui: generated {} OUI entries", entries.len());
}

/// Simple CSV line parser that handles quoted fields.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => {
                in_quotes = true;
            }
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Strip invisible and control characters from an organization name.
///
/// The IEEE CSV contains entries with zero-width spaces, soft hyphens, and
/// other invisible Unicode characters that trigger clippy warnings. We strip
/// anything that is not a visible character or a regular space.
fn sanitize_org_name(name: &str) -> String {
    name.chars()
        .filter(|c| {
            // Keep printable characters. Reject control chars, zero-width
            // spaces (U+200B), zero-width non-joiner (U+200C), zero-width
            // joiner (U+200D), soft hyphen (U+00AD), BOM (U+FEFF), and
            // other format characters.
            !c.is_control()
                && *c != '\u{200B}'
                && *c != '\u{200C}'
                && *c != '\u{200D}'
                && *c != '\u{00AD}'
                && *c != '\u{FEFF}'
                && *c != '\u{FFFD}'
                && *c != '\u{2060}' // word joiner
                && *c != '\u{2028}' // line separator
                && *c != '\u{2029}' // paragraph separator
        })
        .collect::<String>()
        .trim()
        .to_string()
}
