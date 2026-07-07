# Remote access & auto-HTTPS

Wardnet can give itself a public hostname and a real, auto-renewing
HTTPS certificate, so you can reach your admin site, your [user
PWA](/docs/mobile-apps), and your [admin mobile PWA](/docs/mobile-apps)
securely from outside your LAN. There's no external reverse proxy to
run: the daemon terminates TLS itself on port 443, redirects plain
HTTP to HTTPS, and routes each surface by path (`/`, `/admin/`,
`/admin-app/`) under the one hostname. The private key never leaves
the Pi.

Open **Remote access** to set it up.

## Choosing a provider

![Enable remote access, provider choice](/docs/remote-access/enroll-wardnet.png "wide")

Two ways to get a public hostname:

- **Wardnet**, zero-config. Wardnet assigns you a hostname under
  `wardnet.services`, handles the DNS side itself, and you only need a
  Wardnet account to enroll.
- **Your own domain (Cloudflare)**, bring a domain you already
  control. Wardnet manages the `_acme-challenge` DNS record on that
  domain through a Cloudflare API token you provide.

![Your own domain, Cloudflare token form](/docs/remote-access/enroll-cloudflare.png "wide")

## Enrolling with Wardnet

The zero-config path is a short wizard:

1. Enter your Wardnet account email and click **Send code**.
2. Enter the one-time code emailed to you and click **Verify code**.

![Enrollment code entry](/docs/remote-access/enroll-code.png "wide")

3. Pick a hostname slug. Wardnet checks availability as you type and
   suggests a random alternative if you'd rather not think of one:

![Hostname slug picker with live availability check](/docs/remote-access/enroll-slug.png "wide")

4. Click **Enable remote access**.

Behind the scenes Wardnet requests a certificate via ACME's DNS-01
challenge, publishing the challenge record through whichever provider
you picked and waiting on the certificate authority. Once issued, the
daemon hot-swaps it in without a restart.

## Status

Once configured, the status card shows exactly what's live:

![Remote access status card, fully resolved](/docs/remote-access/status.png "wide")

- **Public hostname**, the fully-qualified domain your Wardnet answers
  on.
- **Provider**, Wardnet or Cloudflare.
- **Certificate**, the expiry date, or "Not issued yet" while the
  first one is still being provisioned.
- **Public DNS**, whether the hostname currently resolves to this
  Pi from the outside: **Resolves correctly**, **Points to the wrong
  IP**, **Not yet visible publicly** (still propagating), or **Not
  configured**.

Right after enabling, this card shows an **Issuing certificate…**
banner instead while the DNS-01 challenge and Let's Encrypt round trip
runs, typically well under a minute. Click **Recheck DNS** any time to
manually re-run the resolution check rather than waiting for the next
automatic one.

## Changing or removing

**Change provider** reopens the same form so you can switch Wardnet
and Cloudflare, or register a different hostname, the daemon registers
the new one before tearing down the old.

**Remove remote access** releases the public hostname, deletes the
certificate, and reverts to plain HTTP. It's confirmed before it runs
since it's disruptive to anyone currently reaching Wardnet remotely,
you can always set it up again afterward.
