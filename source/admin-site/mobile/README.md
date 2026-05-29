# @wardnet/admin-mobile (reserved)

Placeholder. Reserved for the future admin mobile bundle (React Native
app). No code lives here yet; this directory exists only so the name is
taken.

## Intent

`source/admin-app/web/` is the browser bundle for the Wardnet admin
product. `source/admin-app/mobile/` will be its mobile sibling — a
distinct React Native bundle, not a responsive layer over the web app.

The mobile bundle will consume `@wardnet/forge-native` for primitives,
just as `web/` consumes `@wardnet/forge-web`. Both bundles share Forge
tokens through [`source/forge/src/tokens.ts`](../../forge/src/tokens.ts),
so design decisions made once propagate to both surfaces.

## When this lights up

This directory is bootstrapped in the same slice as `forge-native/`.
Until then, do not add files here and do not add `mobile` to
`source/admin-app/package.json` workspaces.
