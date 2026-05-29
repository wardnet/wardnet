# @wardnet/forge-native (reserved)

Placeholder. Reserved for the future React Native primitives package that
will mirror `forge-web` for the admin mobile bundle. No code lives here
yet; this directory exists only so the name is taken.

## Intent

`forge-web` ships browser primitives as Radix + Tailwind utility classes
backed by Forge CSS variables. `forge-native` will ship the same surface
to React Native, with each `forge-web` component answered by a
`StyleSheet`-driven equivalent (no className strings, no Radix — RN
accessibility primitives instead).

The cross-platform truth source for both packages is
[`source/forge/src/tokens.ts`](../../forge/src/tokens.ts). `forge-web`
projects those tokens through `styles.css`; `forge-native` will read them
directly into `StyleSheet.create({...})` calls so colours, radii,
densities, and font scales stay in lock-step across web and mobile.

## When this lights up

The package is bootstrapped in the same slice that introduces
`source/admin-app/mobile/`. Until then, do not add files here and do not
add `forge-native` to `source/admin-app/package.json` workspaces.
