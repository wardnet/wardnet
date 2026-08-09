# Privacy Policy

_Last updated: July 18, 2026_

Wardnet is built around one rule: your network's data stays on your
network unless you explicitly opt into something that requires it to
leave. This policy explains what that means in practice, for the
self-hosted daemon, for this marketing site, and for the optional
Premium cloud services.

## Data controller

For Premium and account.wardnet.network, the data controller is Pedro
Gomes, trading as Wardnet, a sole trader (Empresário em Nome
Individual) registered in Portugal, NIF 210741422. Contact:
legal@wardnet.network. A postal address for formal data-protection
requests is available on request via that email.

## 1. The self-hosted daemon

Running Wardnet on your own hardware collects and sends us nothing by
default. Device names, DNS queries, routing rules, and everything else
the daemon manages live in its local database on your device. No
telemetry, analytics, or diagnostics leave your network unless you
explicitly enable OpenTelemetry export (`[otel]` in `wardnet.toml`,
disabled by default) and point it at an endpoint you control, see
[Configuration](/docs/configuration).

## 2. This website

wardnet.network uses **Cloudflare Web Analytics**, a privacy-first,
**cookieless** analytics service. It records only aggregate page views
and referrers, so we can see which pages and articles are read and where
visitors arrive from. It sets no cookies, does no fingerprinting, and
does not track visitors across sites, and it collects no personal data.
It is designed so that neither we nor Cloudflare can identify an
individual visitor. There are no advertising or tracking cookies on this
site.

## 3. Premium and account.wardnet.network

Creating an account and subscribing to Premium involves exactly three
pieces of personal data, nothing more:

- **Your account email**, deleted immediately on account cancellation.
- **Payment data**, held entirely by Stripe on our behalf, we never
  see or store your card details ourselves, deleted immediately on
  account cancellation.
- **Your DDNS record's current IP address**, the IP your dynamic
  hostname currently resolves to, deleted immediately on account
  cancellation.

Private DNS queries made while roaming, and remote-access traffic, pass
through our infrastructure while a subscription is active, but we do not
log or retain them, they're routed, not recorded.

We do not sell your data. We share it only with the sub-processors
listed below and when required by law.

## 4. Sub-processors

These are the third parties that process data on our behalf across
Wardnet's website and the Premium cloud services:

| Sub-processor | Purpose |
| --- | --- |
| Stripe | Payment processing and billing. |
| Hetzner | Hosts the account/billing API and the DDNS/tunnel relay infrastructure behind account.wardnet.network. |
| Cloudflare | DNS for Wardnet's own zones (including the zero-config `wardnet.services` dynamic DNS domain), reverse proxy/DDoS protection in front of account.wardnet.network, and privacy-first, cookieless web analytics for wardnet.network. |

If you bring your own domain via Cloudflare for remote access (see
[remote access](/docs/remote-access)), that's a Cloudflare account you
control directly, not a sub-processor relationship with us.

## 5. Cookies

account.wardnet.network uses a session cookie to keep you signed in.
Our website analytics is cookieless, and we don't use cookies for
advertising or cross-site tracking anywhere.

## 6. Data retention

We hold the three items listed in Section 3 for as long as your
account is active. All three, your account email, your payment data
held by Stripe, and your DDNS record's current IP address, are deleted
immediately on account cancellation. We don't keep a copy afterward.

## 7. Your rights

You can access, correct, or delete your account information at any
time from account.wardnet.network, or by contacting us. Deleting your
account cancels any active Premium subscription and stops all cloud
processing tied to it; it does not affect the self-hosted daemon
running on your own hardware, which was never dependent on the
account to begin with.

## 8. Children's privacy

Wardnet and Premium are not directed at children under 16, and we do
not knowingly collect personal information from them.

## 9. Changes to this policy

We may update this policy from time to time. We'll update the "Last
updated" date above when we do.

## 10. Contact

Questions about this policy, or to exercise a data-access request:
**legal@wardnet.network**.
