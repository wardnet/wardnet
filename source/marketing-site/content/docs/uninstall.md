# Uninstall

Wardnet installs an uninstaller alongside the daemon, so removing it is one
command rather than a checklist you have to get right by hand:

```bash
sudo wardnet-uninstall
```

That removes the daemon, its systemd units, its configuration, the `wardnet`
system user, and the firewall and WireGuard state it created in the kernel. It
**keeps** your database and secrets. Read the two warnings below before you run
it, because one of them is about your whole network losing DNS.

## Read this first: your LAN loses DHCP and DNS

This host is your network's DHCP server and its DNS resolver. The moment
Wardnet is gone, devices on the LAN have no way to get an address and no way to
resolve a name, and they will stay that way until something else takes over.

**Re-enable DHCP on your router before you uninstall,** not after. If you do it
after, you may find yourself with no working DNS on the very machine you were
going to use to log into the router. The uninstaller prints this warning too
and waits for you to confirm, so there is a chance to back out.

## Look before you leap

Every uninstall path supports `--dry-run`, which prints every file, unit,
nftables table, interface and user it would touch, marked as removed or kept,
and then exits without changing anything:

```bash
sudo wardnet-uninstall --dry-run
```

For something that owns port 53 and rewrites your firewall, it is worth the ten
seconds.

## What is kept, and how to destroy it

By default your data survives:

| Path | What it holds | Default | `--purge` |
| --- | --- | --- | --- |
| `/var/lib/wardnet/wardnet.db` | Devices, rules, zones, history | Kept | Deleted |
| `/var/lib/wardnet/secrets/` | WireGuard private keys, backup passphrase, DDNS credentials | Kept | Deleted |
| Everything else | Binary, units, config, logs, firewall state | Deleted | Deleted |

Anything kept is re-owned to `root` on the way out. That matters because the
`wardnet` user is deleted in the same run, and files left behind owned by a
user that no longer exists can be silently inherited by whatever service
happens to be assigned that numeric ID next. If the re-own fails for any
reason, the uninstaller **keeps** the `wardnet` user rather than creating that
exact hazard, and tells you so.

One limit worth knowing: these are the default locations. The uninstaller does
not read `wardnet.toml`, deliberately, so that it still works on a host whose
config is missing or broken. If you moved the database, secret store, or log
directory in that file, remove those paths yourself.

To destroy the data as well:

```bash
sudo wardnet-uninstall --purge
```

`--purge` asks you to type `PURGE` in full rather than accepting a `y`, because
the WireGuard private keys and the backup passphrase cannot be recovered once
they are gone. If there is any doubt, take an
[encrypted backup](/docs/backup-restore) first, it takes a moment and it
restores onto a fresh install.

## Running it non-interactively

The installer is happy to run unattended, because installing is safe. The
uninstaller is the opposite: with no terminal to confirm on and no `--yes`, it
refuses and exits non-zero rather than guessing that you meant it.

```bash
sudo wardnet-uninstall --yes
sudo wardnet-uninstall --purge --yes
```

## Other ways in

Three entry points, all doing the same thing:

```bash
# The script the installer wrote (preferred).
sudo wardnet-uninstall

# The daemon binary directly, if the script is missing.
sudo wardnetd uninstall

# From the install script, including the piped form.
curl -sSL https://wardnet.network/install.sh | sudo bash -s -- --uninstall
```

`wardnet-uninstall` is a thin wrapper that hands over to `wardnetd uninstall`.
The real work lives in the daemon because Wardnet manages nftables over netlink
rather than by shelling out to `nft`, so the `nft` command is not something we
install or can assume exists. The binary can always delete its own firewall
table; a shell script cannot. If the binary is missing or broken, the wrapper
falls back to removing files on its own. It still asks for confirmation, and it
still deletes the firewall table when the `nft` command happens to be installed.
If it is not, the wrapper says plainly that firewall state may remain and exits
non-zero, rather than reporting a clean uninstall it did not achieve.

## Never kill the daemon to stop it

Wardnet holds the hardware watchdog on boards that have one, and disarms it as
part of shutting down cleanly. If you `kill -9` the daemon, that disarm never
happens and **the board reboots about fifteen seconds later.**

Use `sudo systemctl stop wardnetd`, and let the uninstaller do its own stopping.
It always stops the service cleanly first, for exactly this reason.

## After a static-IP install

If you installed with `--static-ip`, the address came from a drop-in at
`/etc/dhcpcd.conf.d/wardnet.conf`. Removing it means this host goes back to
DHCP at its next boot, and it may come back on a different address than the one
you have bookmarked. Note the current address before you reboot.

## What gets removed

For reference, the full inventory:

| Item | Purpose |
| --- | --- |
| `/usr/local/bin/wardnetd`, `wardnetd.old` | The daemon and its rollback copy |
| `/usr/local/libexec/wardnet/` | Post-upgrade migration runner |
| `/etc/wardnet/` | `wardnet.toml` and related config |
| `/var/log/wardnet/` | Log files |
| `/var/lib/wardnet/updates/` | Downloaded release artefacts (removed in both tiers) |
| `/var/lib/wardnet/postupgrade/` | Staged post-upgrade migration binary (removed in both tiers) |
| `/var/lib/wardnet-postupgrade/` | Root-owned migration state, outside the main tree |
| `wardnetd.service`, `wardnetd-rollback.service`, `wardnet-postupgrade.service` | systemd units and their enable symlinks |
| `/etc/sysctl.d/99-wardnet.conf` | Persisted `net.ipv4.ip_forward=1` |
| `/etc/dhcpcd.conf.d/wardnet.conf` | Static IP drop-in, only if you used `--static-ip` |
| `/etc/modules-load.d/wardnet-watchdog.conf` | Hardware watchdog module |
| `wardnet` | The system user the daemon ran as |
| `table inet wardnet` | The nftables table, deleted by name |
| `wg_ward*` | Tunnel interfaces and the inbound remote-access server |

Two notes on that list. IP forwarding stays enabled on the *running* kernel
until you reboot, we only remove the drop-in that made it persist, so nothing
else on the host that depends on forwarding breaks underneath you. And the
nftables table is deleted by name, never with a ruleset flush, so Docker's
rules and any of your own are left exactly as they were.

The uninstaller is safe to re-run. Everything it does tolerates the thing
already being gone, so a partial or interrupted removal is fixed by running it
again. If any step does fail, it lists what is still present and exits
non-zero rather than claiming success.

## What Wardnet never touched

Worth stating, because this is where network tools usually leave a mess:

- **`/etc/resolv.conf` is never modified**, and `systemd-resolved` is never
  disabled. Wardnet serves DNS on port 53 without rewriting the host's own
  resolver configuration, so there is nothing to undo and no way for uninstall
  to leave this machine unable to resolve anything.
- **The static IP was written as a drop-in file we own**, not as an edit to
  your `dhcpcd.conf`. Removing it is deleting our file, not unpicking a
  modification from someone else's.

Both are deliberate. They are the difference between an uninstall that is a
`rm` and one that is an archaeology exercise.

## Reinstalling later

If you kept your data, a fresh install picks up the existing database and
secrets at `/var/lib/wardnet` and comes back with your devices, rules and
tunnels intact. See [Installation](/docs/installation).
