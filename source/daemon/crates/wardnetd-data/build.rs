//! Build script for `wardnetd-data`.
//!
//! Parses the IEEE MA-L (OUI) CSV database at `data/oui.csv` and generates a
//! Rust source file with a static array of OUI-to-manufacturer mappings.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

fn main() {
    generate_oui_data();
}

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

        if i == 0 {
            continue;
        }

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

fn sanitize_org_name(name: &str) -> String {
    name.chars()
        .filter(|c| {
            !c.is_control()
                && *c != '\u{200B}'
                && *c != '\u{200C}'
                && *c != '\u{200D}'
                && *c != '\u{00AD}'
                && *c != '\u{FEFF}'
                && *c != '\u{FFFD}'
                && *c != '\u{2060}'
                && *c != '\u{2028}'
                && *c != '\u{2029}'
        })
        .collect::<String>()
        .trim()
        .to_string()
}
