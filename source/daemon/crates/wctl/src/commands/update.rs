//! `wctl update status | check | install | rollback`.

use wardnet_common::api::{
    InstallUpdateRequest, InstallUpdateResponse, RollbackResponse, UpdateCheckResponse,
    UpdateStatusResponse,
};
use wardnet_common::update::{InstallPhase, UpdateStatus};

use crate::client::Client;
use crate::error::CliError;
use crate::output;

/// `GET /api/update/status`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn status(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: UpdateStatusResponse = client.get("/api/update/status").await?;
    if json {
        return output::print_json(&resp);
    }
    print_status(&resp.status);
    Ok(())
}

/// `POST /api/update/check` — force a manifest refresh.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn check(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: UpdateCheckResponse = client.post_empty("/api/update/check").await?;
    if json {
        return output::print_json(&resp);
    }
    print_status(&resp.status);
    Ok(())
}

/// `POST /api/update/install`, optionally pinning a version.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn install(client: &Client, json: bool, version: Option<&str>) -> Result<(), CliError> {
    let req = InstallUpdateRequest {
        version: version.map(str::to_owned),
    };
    let resp: InstallUpdateResponse = client.post_json("/api/update/install", &req).await?;
    if json {
        return output::print_json(&resp);
    }
    println!("{}", resp.message);
    println!(
        "install {} targeting {}",
        resp.handle.install_id, resp.handle.target_version
    );
    Ok(())
}

/// `POST /api/update/rollback`.
///
/// # Errors
/// Propagates any [`CliError`] from the request.
pub async fn rollback(client: &Client, json: bool) -> Result<(), CliError> {
    let resp: RollbackResponse = client.post_empty("/api/update/rollback").await?;
    if json {
        return output::print_json(&resp);
    }
    println!("{}", resp.message);
    Ok(())
}

fn print_status(s: &UpdateStatus) {
    println!("current version: {}", s.current_version);
    println!(
        "latest version:  {}",
        output::opt(s.latest_version.as_deref())
    );
    println!("update available: {}", s.update_available);
    println!("channel:         {}", s.channel.as_str());
    println!("auto-update:     {}", s.auto_update_enabled);
    println!("phase:           {}", install_phase(&s.install_phase));
    println!(
        "pending version: {}",
        output::opt(s.pending_version.as_deref())
    );
    println!("rollback ready:  {}", s.rollback_available);
    println!("last check:      {}", output::timestamp(s.last_check_at));
    println!("last install:    {}", output::timestamp(s.last_install_at));
}

fn install_phase(phase: &InstallPhase) -> String {
    match phase {
        InstallPhase::Idle => "idle".to_owned(),
        InstallPhase::Checking => "checking".to_owned(),
        InstallPhase::Downloading { bytes, total } => match total {
            Some(total) => format!(
                "downloading ({} / {})",
                output::bytes(*bytes),
                output::bytes(*total)
            ),
            None => format!("downloading ({})", output::bytes(*bytes)),
        },
        InstallPhase::Verifying => "verifying".to_owned(),
        InstallPhase::Staging => "staging".to_owned(),
        InstallPhase::Swapping => "swapping".to_owned(),
        InstallPhase::RestartPending => "restart-pending".to_owned(),
        InstallPhase::Applied => "applied".to_owned(),
        InstallPhase::Failed { reason } => format!("failed: {reason}"),
    }
}
