---
status: accepted
date: 2026-08-03
issue: "#1099 (make LAN devices identifiable)"
---

# ADR: Device identification — a shared vendor catalog, hedged guesses, and no background probing

---

## Context

An admin holding a vendor app that reported a device as `5c:e7:53:4e:ec:db`
could not find it in Wardnet, and could not tell whether it was absent or
merely unrecognisable. On a live 34-device deployment, 22 devices showed the
manufacturer `Randomized MAC` and 12 had no name at all.

Three things were true at once:

1. The OUI was the **only** identification signal, and for 108 IEEE rows it is
   the literal token `Private` — a registrant who paid to hide their name. We
   surfaced that string as if it were a manufacturer.
2. `Randomized MAC` was a **sentinel string written into the `manufacturer`
   column**, conflating *how a device presents itself* with *who built it*.
3. The MAC printed in a vendor app is frequently the device's **Bluetooth**
   address, not the Wi-Fi address it associated with. An admin comparing the
   two is comparing different identifiers with no way to know it.

Adding identification signals means deciding how far Wardnet will go to name a
device — including whether it may send unsolicited traffic to the admin's own
LAN. Those choices are hard to reverse once a data format ships to the field
and a privacy precedent is set, so they are recorded here.

## Decision

### 1. Placeholder IEEE listings are *no match*, not a name

`wardnetd-data/build.rs` drops rows whose organisation name is `Private`, empty,
or `IEEE Registration Authority` (415 rows — MA-M/MA-S parent blocks whose real
assignee sits behind a 28- or 36-bit prefix a 24-bit lookup cannot resolve).
They are absent from the generated table, so `lookup_manufacturer` returns
`None` *by construction* rather than by a filter someone can forget.

The UI renders `None` as "Unknown manufacturer" with a tooltip explaining the
two reasons it can be blank. A bare `-` was the actual failure: it gave the
admin no way to distinguish a Wardnet gap from a vendor's deliberate choice.

### 2. A privacy MAC is a flag, not a manufacturer

`lookup_manufacturer` no longer returns `"Randomized MAC"`. The fact moves to a
dedicated `is_randomized` column and renders as a badge on the address. The
"randomized ⇒ probably a phone" inference survives, keyed off the flag at the
discovery call site rather than off a magic string.

This is backfilled by migration rather than left to churn. A static IoT device
may never be rediscovered, so without a backfill the reported deployment would
have seen no change at all.

### 3. One vendor catalog drives every signal kind

`wardnetd-data/data/vendors.toml` is the single extension point. Each entry
declares any of: OUI overrides, TCP ports, mDNS service types, DHCP option-60
strings. **Adding a manufacturer is a data edit, not a code change** — the
explicit requirement that shaped this design.

The alternative — a lookup table per signal kind — was rejected because a
vendor's marks are one fact about one vendor, and splitting them guarantees the
four tables drift.

### 4. A curated OUI override is a hedged guess, never a fact

We *do* ship overrides for `Private` blocks (`5CE753 → Govee`), because
otherwise the reported device is unidentifiable by construction. But a match is
recorded as `manufacturer_source = 'catalog'` and renders as **"Likely Govee"**,
distinct from an IEEE registrant shown plainly.

This is the uncomfortable one: we are asserting something the registrant paid
IEEE to withhold, from a list we maintain by hand, against blocks that can be
reassigned. Marking the provenance is what makes that acceptable — the admin
can see it is our inference. An inferred name never overwrites an IEEE one.

### 5. Identification never scans in the background

Active probing is the strongest vendor signal and the only one that emits
unsolicited traffic to the admin's own devices. It is exposed **only** as an
explicit per-device "Identify this device" action.

Rejected: a global opt-in that probes every newly discovered device. It is what
the issue originally proposed, but it needs a settings surface, a background
probe path, and it scans devices the admin never asked about. A button is
cheaper to build, trivial to reason about, and its consent is unambiguous —
the admin is standing there holding the vendor app. The probe surface is
bounded by the catalog and asserted small by test.

**Invariant: no code path probes a device without a direct admin action.**

The invariant is **doc-enforced, not machine-enforced.** `auth_context::require_admin()`
does not hold it: a background runner can mint
`AuthContext::Admin { admin_id: Uuid::nil() }` — exactly what the DHCP server
does today to record signals — so the auth gate waves such a caller straight
through. What holds the line is the `# Invariant` doc comment on the
`DeviceProber` trait and review against it. Rejected: a CI grep over callers
(brittle against any indirection, and it would pass a runner that reached the
service through one more hop) and a runtime nil-admin refusal (it would break
the legitimate background signal recorders that already use that context).

**Online-only.** A probe is refused with `409` unless the device has been seen
within `detection.departure_timeout_secs` (300 s), and unless its `last_ip`
parses and is non-globally-routable. The reason is **misattribution, not
privacy**: `last_ip` is last-observation-wins, so probing a departed device
contacts whoever holds that address now, and `set_manufacturer_if_absent` would
write that stranger's vendor onto this device's row permanently — a wrong name
that no later signal can correct, since naming is first-writer-wins. Reusing
the existing departure timeout rather than inventing a second threshold keeps
"present" meaning one thing across the daemon. Remote WireGuard peers on
`10.100.64.0/24` are private, so roaming devices stay probeable.

The probe result is **not persisted**: there is no `last_probed_at` column and
no migration. A reload therefore loses the "we probed and found nothing" fact.
Accepted — the response carries `ports_probed` alongside `answering_ports` so
the admin sees that outcome stated plainly at the moment they ask for it.

### 6. Neighbour matching is ±4 over the full 48-bit MAC

On an exact-MAC miss, devices within ±4 of the searched address are offered as
*possible* matches, visually separate from exact results and labelled with the
Bluetooth-MAC explanation.

The window comes from how Espressif — the chipset behind most affected
smart-home gear — derives Wi-Fi STA / Wi-Fi AP / BT / ETH from one base address
as base+0/+1/+2/+3.

The arithmetic is over the whole 48-bit value, **not the last octet as
originally proposed**. Last-octet subtraction breaks at the byte boundary: it
scores `…:ef:00` and `…:ee:fe` as 254 apart when they are 2, and scores two
unrelated addresses sharing a trailing byte as identical.

## Consequences

- A device with a `Private` OUI and no observed signal now shows *nothing*
  where it previously showed "Private". This is the intended trade: an honest
  blank with an explanation beats confident noise.
- The catalog needs upkeep, and a stale override will confidently mislabel a
  reassigned block. Bounded by rendering catalog matches as "likely".
- Neighbour matching can point at a genuinely unrelated device that happens to
  sit within 4 addresses. Accepted because it is labelled a guess and the
  alternative is the dead end that motivated the issue.
- mDNS observation maps an **IP** to a device, not a MAC. Ambiguous mappings are
  skipped rather than guessed — a wrong attribution is worse than no signal.
- The no-background-scanning invariant survives only as long as reviewers hold
  it. A future runner could call `probe_device` with a nil-admin context and
  nothing in CI or at runtime would stop it. The mitigation is that the trait
  carries the rule where a caller cannot miss it.
- A device that is asleep or has just roamed off the LAN cannot be identified
  at all, and the admin has to wake it and retry. Accepted: a probe of a
  reassigned address is worse than a probe that has to wait.
