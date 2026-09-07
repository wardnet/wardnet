# Device and path diagnostics

Wardnet could describe how a device is *configured* and nothing about how it is
*behaving*. Diagnosing a device that loses connectivity required SSH, five data
sources and two hand-written packet sniffers — and several faults had run
unnoticed for months because nothing was watching. This records the decisions
behind the three-part answer: anomaly detectors, a per-device connectivity
timeline, and per-egress-path health probes.

## A path probe completes a transfer, not just a connect

The probe establishes a TCP connection to the configured probe endpoint **and
then completes a TLS handshake plus a small HTTPS request**, recording success
and latency for each stage separately.

The obvious design — a bare `connect()` — cannot see the failure that motivated
the work. A three-way handshake is three small packets, so under an MTU/MSS
asymmetry it succeeds while every real connection stalls on the first
full-size segment. A connect-only probe would have reported the path healthy
throughout the outage it exists to explain.

Two stages make the diagnosis fall out of the data rather than out of a guess:

| `connect` | `transfer` | Meaning |
|-----------|------------|---------|
| fail      | —          | The path carries no traffic at all. |
| ok        | fail       | Establishes, then stalls on large segments — the MSS/MTU signature. |
| ok        | ok         | The path works end to end. |

These are two anomaly types, `EgressPathUnreachable` and `EgressPathDegraded`,
not one type with the stage in `details`. An `AnomalyType` carries the
remediation hint, and "your tunnel is dead" and "your tunnel silently drops
large packets" send an admin to entirely different places. Burying that in a
details blob puts the most useful half of the diagnosis where the UI does not
show it.

## Two invariants

**Probing our own egress paths is not probing a device.** ADR 0025's rule —
*nothing probes a device without a direct admin action* — governs traffic aimed
at a household device. An egress path is Wardnet's own uplink; measuring it
sends packets to a public endpoint through our own interface and touches no
device. The distinction is stated here so the next reader does not have to
re-derive it.

**A failing path probe is an anomaly and never an input to `HealthMonitor`.**
This is ADR 0030's rule for a published app's reachability probe, applied for
the same reason: the health-gated soft watchdog restarts `wardnetd`, so feeding
it a signal that depends on a third party means a flaky VPN — or a cloud
outage — restarts the daemon. Enforced structurally: the prober does not
implement `HealthCheck` and is never registered with the monitor.

Only paths whose tunnel is `Up` are evaluated (direct always is). A `Down`
tunnel is already `TunnelUnhealthy`'s subject, and alerting twice for one fault
helps nobody. The new anomalies therefore mean something narrower and more
useful: *locally healthy by every measure we have, yet functionally broken.*

## Egress path is not routing target

A **routing target** is a policy choice and may be `default`; an **egress path**
is a concrete interface packets leave by. `default` is a deferral, so probing it
is incoherent — the series would silently change meaning when the gateway policy
changed. Only an egress path can have a socket bound to it, which is exactly
what makes it measurable. See CONTEXT.md.

## `device_events` exists, and is capped two ways

The timeline was specified as needing no new collection. That is true of only
two of its five inputs: DHCP events (`dhcp_lease_log`) and DNS result mix
(`dns_query_log`). Presence transitions, IP changes and zone/binding changes are
published as `WardnetEvent`s and dropped; conntrack flushes are an INFO log
line. Without somewhere to put them the timeline cannot answer "did it depart?",
which is the question the motivating example turns on.

So one append-only `device_events` table, written by a listener on the existing
event bus. It follows `dhcp_lease_log`: `mac` denormalised, and **no foreign key
to `devices`** — ADR 0034 already rejected that for `dns_query_log` because
`devices.id` is `TEXT` and device retention deletes rows the log must outlive.

Retention is **both** a 30-day age cap and a per-device row cap. Age alone is
not enough: measured on a live box the table costs ~2.5 MB/month in steady
state, but a single misbehaving bridge sustaining the observed 982 changes/hour
would reach ~155 MB. The pathological case is exactly when the timeline is most
needed and when it grows 50×, so the row cap bounds the worst case while the age
cap governs normal operation. The same 30-day prune is applied to
`dhcp_lease_log`, which until now was pruned by nothing at all.

## Detector thresholds are derived, not guessed

Every threshold below was chosen against a week of data from a live 55-device
box rather than picked by feel.

**`DhcpRenewalStorm` is derived from the configured lease.** A fixed
renewals-per-day threshold is not merely imprecise, it inverts: at the default
86400s lease a client should renew twice a day, but at a 3600s lease `T1` is 30
minutes and a healthy client renews 48 times a day — so a constant tuned for the
former fires for *every* device on the latter. The threshold is therefore
`20 × 86400/(lease/2)`, read from config at sweep time, with an absolute floor
of 12/day so a very long lease cannot make a handful of retries alertable.
(Wardnet does not send options 58/59, so clients use the RFC 2131 default
`T1 = lease/2`; closing that gap is issue #1339.)

A 24-hour window is deliberate. Measured hourly, `≥4 renewals/hour` fires for 24
different MACs in a week — almost all single-hour blips from phones waking. The
signal is persistence, not peak rate, and a 24-hour window captures that without
a second "sustained for N hours" condition to tune.

**`DeviceAddressChurn` is a hardcoded 50 changes per MAC per 24h**, because the
measured distribution is cleanly bimodal: four devices above 238/day, seventeen
at or below 20/day, and *nothing at all* between 21 and 100. The threshold sits
in an empty band, so it is robust rather than tuned. It is deliberately **not**
config-derived — discovery observes ARP, not just DHCP, so tying it to the lease
would invent a relationship that does not exist.

This is kept separate from the #886 in-memory flap guard, which a future reader
will be tempted to merge it into. They answer different questions: the guard is
*protective* (a 60s window, counting distinct addresses, silently distrusting a
MAC), this is *diagnostic* (a 24h window, counting transitions, telling the
admin). A MAC ping-ponging between exactly two addresses 238 times a day is
invisible to the guard and is precisely this detector's target.

## The timeline reads the query log directly

Per-device DNS *volume* is already in `stats` (`dns.queries.by_device`), and the
result mix is there globally (`dns.queries{outcome}`) — but not the cross
product. Adding a `{device_id, outcome}` metric was rejected: it would start from
empty and show nothing retrospectively, which is the opposite of what a
diagnostic view needs. `dns_query_log` already holds the data at 7-day
retention, and the per-minute mix for the busiest device over 24h measures
**39.5 ms** on the `device_id` index. The repository does the `lk_*` joins, so
ADR 0034's rule that nothing above `wardnetd-data` knows lookup tables exist
still holds.
