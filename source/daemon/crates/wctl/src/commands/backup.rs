//! `wctl backup status | export | import | snapshots`.

use std::io::Read;
use std::path::Path;

use wardnet_common::api::{
    ApplyImportRequest, ApplyImportResponse, BackupStatusResponse, ExportBackupRequest,
    ListSnapshotsResponse, RestorePreviewResponse,
};
use wardnet_common::backup::{BackupStatus, MIN_PASSPHRASE_LEN, RestorePhase};

use crate::client::Client;
use crate::error::CliError;
use crate::output;

/// `GET /api/backup/status`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn status(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: BackupStatusResponse = client.get("/api/backup/status").await?;
    if json {
        return output::print_json(&resp);
    }
    println!("{}", backup_state(&resp.status));
    Ok(())
}

/// `POST /api/backup/export` — write the encrypted bundle to `out`.
///
/// # Errors
/// [`CliError::Usage`] for a too-short passphrase, [`CliError::Io`] on a write
/// failure, or any [`CliError`] from the request.
pub async fn export(
    client: &Client,
    json: bool,
    out: &str,
    passphrase_file: Option<&str>,
) -> Result<(), CliError> {
    let passphrase = read_passphrase(passphrase_file, true)?;
    if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(CliError::Usage(format!(
            "passphrase must be at least {MIN_PASSPHRASE_LEN} characters"
        )));
    }

    let req = ExportBackupRequest { passphrase };
    let bytes = client.post_json_bytes("/api/backup/export", &req).await?;
    std::fs::write(out, &bytes)
        .map_err(|e| CliError::Io(format!("cannot write bundle to {out}: {e}")))?;

    let written = bytes.len() as u64;
    if json {
        return output::print_json(&serde_json::json!({ "out": out, "bytes": written }));
    }
    println!("wrote {} to {out}", output::bytes(written));
    Ok(())
}

/// `POST /api/backup/import/preview` then `/apply` — restore a bundle.
///
/// The preview step is what validates the passphrase and compatibility before
/// anything on disk changes; only a compatible preview is applied.
///
/// # Errors
/// [`CliError::Io`] if the bundle cannot be read, [`CliError::Protocol`] if the
/// bundle is incompatible, or any [`CliError`] from the requests.
pub async fn import(
    client: &Client,
    json: bool,
    bundle_path: &str,
    passphrase_file: Option<&str>,
) -> Result<(), CliError> {
    let passphrase = read_passphrase(passphrase_file, false)?;
    let data = std::fs::read(bundle_path)
        .map_err(|e| CliError::Io(format!("cannot read bundle {bundle_path}: {e}")))?;
    let file_name = Path::new(bundle_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bundle.wardnet.age")
        .to_owned();

    let form = reqwest::multipart::Form::new()
        .part(
            "bundle",
            reqwest::multipart::Part::bytes(data).file_name(file_name),
        )
        .text("passphrase", passphrase);
    let preview: RestorePreviewResponse = client
        .post_multipart("/api/backup/import/preview", form)
        .await?;

    if !preview.compatible {
        let reason = preview
            .incompatibility_reason
            .unwrap_or_else(|| "incompatible bundle".to_owned());
        return Err(CliError::Protocol(format!(
            "bundle cannot be restored: {reason}"
        )));
    }

    if !json {
        println!(
            "restoring bundle from host {} (schema v{})",
            preview.manifest.host_id, preview.manifest.schema_version
        );
        for file in &preview.files_to_replace {
            println!("  will replace: {file}");
        }
    }

    let apply_req = ApplyImportRequest {
        preview_token: preview.preview_token,
    };
    let applied: ApplyImportResponse = client
        .post_json("/api/backup/import/apply", &apply_req)
        .await?;

    if json {
        return output::print_json(&applied);
    }
    println!("restore applied; restart the daemon to load the new state");
    for snap in &applied.snapshots {
        println!("  snapshot: {} ({})", snap.path, snap.kind.as_str());
    }
    Ok(())
}

/// `GET /api/backup/snapshots`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn snapshots(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: ListSnapshotsResponse = client.get("/api/backup/snapshots").await?;
    if json {
        return output::print_json(&resp);
    }
    if resp.snapshots.is_empty() {
        println!("no snapshots");
        return Ok(());
    }
    println!("{:<10}  {:<12}  {:<26}  PATH", "KIND", "SIZE", "CREATED");
    for snap in &resp.snapshots {
        println!(
            "{:<10}  {:<12}  {:<26}  {}",
            snap.kind.as_str(),
            output::bytes(snap.size_bytes),
            snap.created_at.to_rfc3339(),
            snap.path,
        );
    }
    Ok(())
}

/// Obtain the export/import passphrase.
///
/// From `--passphrase-file` (a path, or `-` for stdin) when given, otherwise a
/// no-echo interactive prompt. When `confirm` is set (export), the prompt is
/// asked twice and the two must match.
fn read_passphrase(passphrase_file: Option<&str>, confirm: bool) -> Result<String, CliError> {
    if let Some(source) = passphrase_file {
        let raw = if source == "-" {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| CliError::Io(format!("reading passphrase from stdin failed: {e}")))?;
            buf
        } else {
            std::fs::read_to_string(source)
                .map_err(|e| CliError::Io(format!("cannot read passphrase file {source}: {e}")))?
        };
        // Strip only a trailing newline (and CR) that `echo`/editors append —
        // interior characters are part of the passphrase.
        return Ok(raw.trim_end_matches(['\r', '\n']).to_owned());
    }

    let pass = rpassword::prompt_password("Passphrase: ")
        .map_err(|e| CliError::Io(format!("reading passphrase failed: {e}")))?;
    if confirm {
        let again = rpassword::prompt_password("Confirm passphrase: ")
            .map_err(|e| CliError::Io(format!("reading passphrase failed: {e}")))?;
        if pass != again {
            return Err(CliError::Usage("passphrases do not match".to_owned()));
        }
    }
    Ok(pass)
}

fn backup_state(status: &BackupStatus) -> String {
    match status {
        BackupStatus::Idle => "idle".to_owned(),
        BackupStatus::Exporting => "exporting".to_owned(),
        BackupStatus::Importing { phase } => format!("importing ({})", restore_phase(phase)),
        BackupStatus::Failed { reason } => format!("failed: {reason}"),
    }
}

fn restore_phase(phase: &RestorePhase) -> String {
    match phase {
        RestorePhase::Idle => "idle".to_owned(),
        RestorePhase::Validating => "validating".to_owned(),
        RestorePhase::StoppingRunners => "stopping-runners".to_owned(),
        RestorePhase::BackingUp => "backing-up".to_owned(),
        RestorePhase::Extracting => "extracting".to_owned(),
        RestorePhase::Migrating => "migrating".to_owned(),
        RestorePhase::RestartingRunners => "restarting-runners".to_owned(),
        RestorePhase::Applied => "applied".to_owned(),
        RestorePhase::Failed { reason } => format!("failed: {reason}"),
    }
}
