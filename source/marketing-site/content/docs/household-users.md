# Household users

Wardnet started with a single admin account. It now has a **user
directory**: everyone in your home can have their own login, and you
decide who can change settings and who just uses the network.

Accounts live **on your Wardnet**, not in anybody's cloud. No company
holds the keys to your home network, and nobody outside it can be
granted a way in. The trade is worth stating plainly: because there is
no external account system, there is also nobody to email for a password
reset. Recovery is local, and covered at the end of this page.

## Roles

There are exactly two.

- **Admin** — can change anything: routing, zones, DNS, tunnels, other
  users. An admin is the same thing the original single account always
  was; there is no reduced tier of admin.
- **Member** — uses the network and manages their own profile and
  password. They cannot reach the admin settings.

Wardnet always keeps **at least one enabled admin**. The controls to
disable, demote, or delete the last one are switched off, because a
Wardnet with no admin cannot be administered, and there is no way in
from outside your network to fix it.

## Adding someone

Open **Household → Users** and choose **Add user**. You give a name, an
optional email, and a role.

The new account is created **empty**: it has no password and cannot sign
in yet. That is deliberate. You never set another person's password,
because then you would know it. Instead you send them an invitation and
they choose their own.

### Invitations

On the person's page, choose **New invitation**. Wardnet generates a
one-time code and shows it to you **once**.

Copy it and pass it to them however you like — a message, a note, out
loud across the kitchen. Then:

- It is good for **72 hours**.
- It can be redeemed **once**. After that it is dead, and the page shows
  when it was used.
- Wardnet stores only a scrambled fingerprint of it, never the code
  itself. If you lose it before sending it, you cannot look it up —
  issue another one and the old one keeps its own separate life until it
  expires (or you revoke it).

If you issued one by mistake, **Revoke** it.

Wardnet also shows a ready-made **link** next to the code. Sending that
is usually easier than asking somebody to retype 32 characters — it opens
the redemption page with the code already filled in.

### Redeeming one

The person opens the link, or goes to **/admin/redeem** and pastes the
code in. They choose a password, confirm it, and that is the account
live. From then on they sign in normally.

They never need an account anywhere else, and you never learn what they
chose.

## Passwords

Every account has a **Wardnet password**, and it can never be removed.

This is the floor the whole design rests on. It needs no internet
connection, no certificate, and no outside company, so it keeps working
during an outage — exactly when you are most likely to need to get into
your router.

Only the account holder can change their own password, and they must
type the current one to do it. Changing it signs that account out
**everywhere, including the browser you changed it in** — you will be
asked to sign in again with the new password. That is deliberate: a
password change is usually a response to "somebody may know this", and
leaving any session alive would make the change cosmetic.

Admins have no button to set somebody else's password. If a member is
locked out, send them a fresh invitation — the same mechanism that
enrolled them will let them set a new password without you learning it.

## Signing in with Google or GitHub

Optional, and off until you set it up. When enabled, people can sign in
with a Google or GitHub account instead of typing a password.

Wardnet does not use a shared Wardnet-branded app for this. **Your
household registers its own application** with the provider, so the
connection is between your Wardnet and your provider account, with
nobody in between.

### Requirements

You need **Remote access** working first, so your Wardnet has a public
address. Google and GitHub have to be able to send the browser back to
your box after sign-in, and without a public hostname there is nowhere
to send it. The setup page says so rather than offering a button that
would fail.

### Setting it up

1. Go to **Household → Sign-in methods**.
2. Copy the **Redirect URI** shown for the provider. It looks like:

   ```
   https://your-name.wardnet.network/api/auth/oauth/google/callback
   ```

   Paste it into your provider's app configuration **exactly** as shown.
   Sign-in fails at the provider if it differs by even one character.
   This URI is also the one thing here you should avoid changing later:
   every household registers it by hand, so altering it silently breaks
   every existing registration.

3. Create an OAuth app at your provider ([Google Cloud
   Console](https://console.cloud.google.com/apis/credentials) or
   [GitHub developer settings](https://github.com/settings/developers))
   and copy its **Client ID** and **Client secret** back into Wardnet.
4. Turn the provider **on**.

Once a provider is on, its button appears on the sign-in page underneath
the username and password fields. Until then the sign-in page shows no
trace of it — a button that cannot work is worse than no button.

The client secret is write-only. Wardnet stores it and never shows it
again — the page reports only whether one is present. To change other
settings later, leave the secret field blank and it keeps the one you
saved.

### Linking an account to a person

A Google or GitHub account has to be **linked to an existing Wardnet
user** before it can sign anyone in. An unrecognised account is refused.

Wardnet never creates an account from a federated login, which would
otherwise mean anyone with a Google account could make themselves a
login on your home network. You add the person first; the link comes
after.

You can unlink a provider from someone's page at any time. Their Wardnet
password still works — that is the point of it never being removable.

## Whose device is whose

On a device's page you can say who it **belongs to**, so lists and
activity read as "Ana's laptop" rather than a MAC address.

This is a **label, and only a label**. It grants nothing. Assigning your
phone to an admin does not make your phone an admin, and assigning it to
a member does not restrict it. What a device may do is decided by its
zone and its rules, whoever owns it.

The reason is worth knowing: Wardnet recognises a device by the address
it is using on your network, which is not proof of anything much. If
ownership carried an owner's privileges, then imitating an address would
be enough to become an admin. So it never does. Signing in is what
grants access, and signing in always takes a credential.

## Deleting someone

Deleting a user removes their sign-in credentials and any invitations.

Their **devices are kept** and simply become unassigned. Deleting a
person must not delete the household's hardware, and it does not.

Disabling is the gentler option and is instant: every session that
account has open is deleted immediately, so it takes effect on their
next click rather than whenever a session happens to expire. Their
devices and settings are untouched, and enabling them again restores
access.

## If you are locked out

Because accounts are local to your Wardnet, there is no "forgot
password" email and no support account that can let you back in. That is
the same property that means nobody outside your home can be granted
access — it cuts both ways, honestly.

What to do:

- **Another admin can help.** They cannot read your password, but they
  can send you a fresh invitation, which lets you set a new one. This is
  the ordinary path, and the reason to keep **two** admin accounts in a
  household.
- **No admin can sign in.** You will need physical access to the machine
  running Wardnet. See [Configuration](/docs/configuration) for the
  admin bootstrap settings the daemon reads at startup.

Keeping a second admin account is the single thing that turns a lockout
from an afternoon into a minute.
