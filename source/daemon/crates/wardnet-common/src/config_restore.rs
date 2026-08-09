//! Which config keys a backup bundle is allowed to change.
//!
//! Restoring a bundle replaces `/etc/wardnet/wardnet.toml` wholesale, and
//! the systemd unit grants `ReadWritePaths=/etc/wardnet` so that write
//! succeeds. That makes the config file reachable from an admin session:
//! export a bundle, unpack it with the passphrase you chose, edit the TOML,
//! repack, restore, restart.
//!
//! Some keys must not be reachable that way. `[update] allow_edge_channel`
//! is the clearest case — ADR-0023 states that putting a box on the edge
//! channel requires root *on that box*, so an admin session (or a stolen
//! one) cannot opt it into unvetted builds. `[update] manifest_base_url` is
//! the same shape and more practically harmful: repoint it at a server that
//! never publishes a release and the box silently stops patching itself
//! while the UI still reports a healthy, up-to-date state.
//!
//! [`preserve_deploy_time_keys`] closes that off by taking every key in
//! [`DEPLOY_TIME_ONLY_KEYS`] from the **live** config rather than the
//! bundle's. Beyond the security property this is the more honest semantic:
//! a bundle restores your data and your settings, not the deployment posture
//! of the machine it lands on.
//!
//! ### Fidelity
//!
//! The merge is untyped — it works on [`toml::Table`], never on
//! [`ApplicationConfiguration`] — so the restored file keeps the shape the
//! operator wrote. Round-tripping through the typed struct would emit every
//! defaulted field, turning the small file `install.sh` writes into a full
//! dump of the config surface.
//!
//! When nothing needs changing, the bundle's bytes are passed through
//! **verbatim**, comments and formatting intact. Re-serialising a
//! `toml::Table` loses both, and that is a real cost on a file
//! `install.sh` writes with explanatory comments — so it is only paid when
//! a deploy-time key actually differs, which on the ordinary
//! restore-onto-the-same-box path it never does.
//!
//! ### Loadability
//!
//! Untyped does not mean unchecked. `ApplicationConfiguration` is
//! `deny_unknown_fields`, and a restore is followed by a mandatory restart —
//! so a bundle carrying an unknown section or a wrongly-typed value would
//! write cleanly and then leave the daemon failing to start, with no UI left
//! to fix it from. Both [`check_bundle_config`] and
//! [`preserve_deploy_time_keys`] therefore require that what they are handed
//! (and what they produce) actually deserialises.
//!
//! That is a check on *shape*, not on values. A config can deserialise and
//! still be one the daemon cannot run on — `[health] refresh_interval_secs =
//! 0` panics the tick loop, `server.host = "nonsense"` fails to parse as a
//! socket address. Nothing here rules those out, because they are the same
//! hazard a hand-edited `/etc/wardnet/wardnet.toml` carries and the fix
//! belongs with the fields themselves, not with restore. What restore
//! guarantees is narrower and worth stating plainly: it will not write a
//! file this daemon cannot *parse*.

use crate::config::ApplicationConfiguration;
use toml::{Table, Value};

/// Config keys taken from the live machine on restore, never from the
/// bundle. Dotted TOML paths; a path naming a table covers its whole
/// subtree.
///
/// Four things land a key here:
///
/// 1. **Code provenance and the update path.** Which builds this box will
///    install, where it fetches them from, and whether it checks at all.
///    This is the [`allow_edge_channel`](crate::config::UpdateConfig::allow_edge_channel)
///    gate and everything that can stand in for it — a one-second
///    `http_timeout_secs` denies patching just as effectively as a dead
///    `manifest_base_url`.
/// 2. **Filesystem locations pinned by the systemd sandbox.** The daemon
///    can only write the paths `wardnetd.service` grants; a bundle that
///    repoints the database or the log file at somewhere else breaks the
///    box, and in the log's case does so silently.
/// 3. **Hardware identity of this box.** Which NIC is the LAN side, which
///    watchdog device exists, whether the board has one at all. These
///    describe the machine, not the user's preferences, and a bundle
///    restored onto different hardware carries the wrong answers.
/// 4. **Test-harness overrides that are never set in production.** Each
///    one redirects the daemon at a mock: stubbed tunnel backends, a
///    stand-in `NordVPN` API, a stand-in wardnet-cloud gateway. Reachable
///    from a bundle, they are a way to point a live box at a server the
///    attacker controls.
///
/// Adding a field to any config section covered here without deciding which
/// side of the line it falls on fails
/// `tests::config_restore::every_config_key_is_classified`.
pub const DEPLOY_TIME_ONLY_KEYS: &[&str] = &[
    // 1. Code provenance and the update path.
    "update.allow_edge_channel",
    "update.manifest_base_url",
    "update.check_interval_secs",
    "update.http_timeout_secs",
    "update.require_signature",
    "update.live_binary_path",
    "update.staging_dir",
    // 2. Filesystem locations pinned by the systemd sandbox.
    "database.provider",
    "database.connection_string",
    "logging.path",
    // Whole table: the secret store's location is machine-local, and the
    // fields under it vary by provider variant. A future provider's
    // connection settings belong on this side of the line by construction.
    "secret_store",
    "pidfile_path",
    // 3. Hardware identity of this box.
    "network.lan_interface",
    "watchdog.enabled",
    "watchdog.device_path",
    // 4. Test-harness overrides, never set in production.
    "test.stub_tunnel_backends",
    "vpn_providers.nordvpn_api_url",
    "ddns_wardnet.gateway_url",
    "ddns_wardnet.region_gateway_url",
    "ddns_wardnet.region_health_url",
];

/// Why a restore could not compute the config it should write.
#[derive(Debug, thiserror::Error)]
pub enum ConfigRestoreError {
    /// The config inside the bundle is not UTF-8, so it is not a config
    /// file at all.
    #[error("the config in the backup bundle is not valid UTF-8: {0}")]
    BundleNotUtf8(std::str::Utf8Error),
    /// The config inside the bundle is not valid TOML, or is not a config
    /// this daemon can load — an unknown section, a wrongly-typed value.
    /// Writing it would leave a daemon that cannot start.
    #[error("the config in the backup bundle is not one this daemon can load: {0}")]
    BundleConfig(toml::de::Error),
    /// The live config on disk is not parseable. The daemon parsed it at
    /// startup, so this means it changed underneath us — and with no live
    /// values to read, the deploy-time keys cannot be preserved. Refuse
    /// rather than let the bundle's copy through.
    #[error("the live config is not valid TOML, so deploy-time keys cannot be preserved: {0}")]
    LiveConfig(toml::de::Error),
    /// The merge produced something this daemon could not load. Only
    /// reachable when the live config is itself not a loadable config, since
    /// the bundle's copy is checked first.
    #[error("merging the live config's deploy-time keys produced a config that will not load: {0}")]
    MergedConfig(toml::de::Error),
    /// Re-serialising the merged table failed.
    #[error("failed to re-serialise the merged config: {0}")]
    Serialise(#[from] toml::ser::Error),
}

impl ConfigRestoreError {
    /// Whether the bundle is at fault, as opposed to the machine being
    /// restored onto. Callers surface the first as a rejected bundle and the
    /// second as an internal failure.
    #[must_use]
    pub const fn blames_the_bundle(&self) -> bool {
        matches!(self, Self::BundleNotUtf8(_) | Self::BundleConfig(_))
    }
}

/// Check that a bundle's config is one this daemon could actually load, and
/// return it as text.
///
/// Split out from [`preserve_deploy_time_keys`] so the restore *preview* can
/// reject an unusable bundle while nothing is at stake, rather than letting
/// the operator commit to a restore that fails — or worse, one that writes a
/// config the daemon then cannot start from.
///
/// # Errors
///
/// [`ConfigRestoreError::BundleNotUtf8`] or
/// [`ConfigRestoreError::BundleConfig`].
pub fn check_bundle_config(bundle: &[u8]) -> Result<&str, ConfigRestoreError> {
    let text = std::str::from_utf8(bundle).map_err(ConfigRestoreError::BundleNotUtf8)?;
    toml::from_str::<ApplicationConfiguration>(text).map_err(ConfigRestoreError::BundleConfig)?;
    Ok(text)
}

/// The config a restore should write, plus what it had to override.
#[derive(Debug, Clone)]
pub struct RestoredConfig {
    /// TOML to write to the live config path.
    pub toml: String,
    /// Deploy-time-only keys whose value in the bundle differed from the
    /// live machine's and were therefore reset to the live value (or
    /// dropped, where the live config left them at their default).
    ///
    /// Empty on every ordinary restore. A non-empty list is worth logging:
    /// it is either a bundle from a differently-deployed box or somebody
    /// editing a bundle to change this one's deployment posture.
    pub overridden: Vec<&'static str>,
}

/// Merge `bundle` over the live config, keeping [`DEPLOY_TIME_ONLY_KEYS`]
/// at their live values.
///
/// A live key that is absent (left at its compiled-in default) is removed
/// from the bundle's copy rather than carried across, so the restored file
/// falls back to the same default. `live` may be empty — that is the
/// no-config-file case, where every deploy-time key resolves to its default.
///
/// The result is checked to deserialise before it is returned, so a restore
/// cannot write a file this daemon would fail to parse. Field *values* are
/// not validated — see the module docs.
///
/// # Errors
///
/// Returns [`ConfigRestoreError`] when either side is not valid TOML, when
/// either side (or the merge of them) is not a config this daemon can load,
/// or when the merged table cannot be serialised.
pub fn preserve_deploy_time_keys(
    live: &str,
    bundle: &str,
) -> Result<RestoredConfig, ConfigRestoreError> {
    check_bundle_config(bundle.as_bytes())?;
    let live_table: Table = live.parse().map_err(ConfigRestoreError::LiveConfig)?;
    let bundle_table: Table = bundle.parse().map_err(ConfigRestoreError::BundleConfig)?;

    let mut merged = bundle_table.clone();
    let mut overridden = Vec::new();

    // An absent key still has a value — its compiled-in default — so the two
    // sides are compared on what they would *resolve to*, not on whether they
    // spell it out. Otherwise a bundle that writes `allow_edge_channel =
    // false` where the live file simply omits it reads as a change: the file
    // gets re-serialised (losing the operator's comments) and the log accuses
    // a perfectly ordinary bundle of trying to move the box's posture.
    //
    // The mirror case is a deliberate trade, not an oversight: where the live
    // file spells out a value that equals the default and the bundle omits
    // it, the restored file omits it too. The box's effective config is
    // unchanged, which is what this function promises — what is lost is the
    // record that the operator pinned it, so a later release that changes
    // that default would move the box with the rest. Keeping the pin would
    // mean re-serialising (and dropping the bundle's comments) for a restore
    // that changes nothing, which is the worse of the two.
    let defaults = default_config_table();

    for key in DEPLOY_TIME_ONLY_KEYS {
        let path: Vec<&str> = key.split('.').collect();
        let default_value = value_at(&defaults, &path);
        let live_value = value_at(&live_table, &path);
        let bundle_value = value_at(&bundle_table, &path);
        if live_value.or(default_value) == bundle_value.or(default_value) {
            continue;
        }
        overridden.push(*key);
        match live_value {
            Some(value) => set_value(&mut merged, &path, value.clone()),
            // The live file leaves this key at its default; drop the bundle's
            // copy rather than writing the default out, so the restored file
            // stays as small as the one the operator had.
            None => remove_value(&mut merged, &path),
        }
    }

    // Nothing to change: hand back the bundle's own bytes so its comments
    // and key order survive a restore untouched. Already checked loadable
    // above, so it needs no second pass.
    if overridden.is_empty() {
        return Ok(RestoredConfig {
            toml: bundle.to_owned(),
            overridden,
        });
    }

    let toml = toml::to_string_pretty(&merged)?;
    toml::from_str::<ApplicationConfiguration>(&toml).map_err(ConfigRestoreError::MergedConfig)?;
    Ok(RestoredConfig { toml, overridden })
}

/// The compiled-in defaults as a table, for resolving what an omitted key
/// would mean. An empty table on the (unreachable) serialisation failure —
/// that degrades the comparison to "written or not", never to a wrong
/// preserved value.
fn default_config_table() -> Table {
    Value::try_from(ApplicationConfiguration::default())
        .ok()
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default()
}

/// Resolve a dotted path to the value it names, or `None` if any segment
/// is missing or is not a table.
fn value_at<'a>(table: &'a Table, path: &[&str]) -> Option<&'a Value> {
    let (last, parents) = path.split_last()?;
    let mut current = table;
    for segment in parents {
        current = current.get(*segment)?.as_table()?;
    }
    current.get(*last)
}

/// Write `value` at a dotted path, creating intermediate tables.
///
/// A non-table sitting where a parent table belongs is replaced rather than
/// tripping an assertion. Nothing can produce that today — a bundle with
/// `update = "surprise"` is rejected by [`check_bundle_config`] before it
/// gets here — but the helper stays total so the merge cannot be turned into
/// a panic by a future caller that skips the check.
fn set_value(table: &mut Table, path: &[&str], value: Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut current = table;
    for segment in parents {
        let entry = current
            .entry((*segment).to_owned())
            .or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        current = entry
            .as_table_mut()
            .expect("entry was just coerced to a table");
    }
    current.insert((*last).to_owned(), value);
}

/// Remove the value at a dotted path, pruning any table the removal leaves
/// empty so a restore does not litter the file with bare `[section]`
/// headers.
fn remove_value(table: &mut Table, path: &[&str]) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        table.remove(*head);
        return;
    }
    let Some(child) = table.get_mut(*head).and_then(Value::as_table_mut) else {
        return;
    };
    remove_value(child, rest);
    if child.is_empty() {
        table.remove(*head);
    }
}
