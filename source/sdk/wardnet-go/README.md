# Wardnet Go SDK

Go client for the [Wardnet](https://wardnet.network) daemon API — the same
HTTP API the admin UI and the `wctl` CLI speak.

```go
import wardnet "wardnet.network/go"
```

```sh
go get wardnet.network/go
```

## Where this code lives

The source of truth is **[wardnet/wardnet](https://github.com/wardnet/wardnet)**,
under `source/sdk/wardnet-go/`. It is developed there alongside the daemon so
that an API change and its SDK update land in one commit.

**[wardnet/wardnet-go](https://github.com/wardnet/wardnet-go) is a generated,
read-only mirror.** It exists only so `wardnet.network/go` resolves: Go locates
a module by subtracting the import path from the repository root, which means a
module named `wardnet.network/go` has to sit at the *root* of the repository it
is fetched from. The mirror is force-pushed from the monorepo at release time
with `git subtree split`.

So:

- **Issues and pull requests belong on
  [wardnet/wardnet](https://github.com/wardnet/wardnet/issues).** Anything
  opened against the mirror is against a copy, and the next release overwrites
  it.
- Commits pushed directly to the mirror are lost on the next release.

## Usage

```go
package main

import (
	"context"
	"fmt"
	"log"

	wardnet "wardnet.network/go"
)

func main() {
	client, err := wardnet.New(
		"https://wardnet.local",
		wardnet.WithToken("…"),
	)
	if err != nil {
		log.Fatal(err)
	}

	devices, err := client.Devices.List(context.Background())
	if err != nil {
		log.Fatal(err)
	}
	for _, d := range devices {
		fmt.Println(d.ID, d.LastIP)
	}
}
```

The package is layered: `internal/rest` is generated from the daemon's
`docs/openapi.json` with oapi-codegen and stays unexported, while the public
surface is hand-written. That split is deliberate — regenerating the client
changes the internal types, and the mapping code stops compiling until it is
updated, which is what catches API drift at build time instead of at runtime.

## License

MIT — see [LICENSE](LICENSE). The Wardnet daemon itself is GPL-3.0-or-later;
this client links none of it and carries no copyleft obligation.
