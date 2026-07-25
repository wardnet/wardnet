// Package wardnet is the Go client SDK for the wardnet network-privacy
// gateway.
//
// It offers a hand-crafted, domain-oriented API over the daemon's HTTP
// interface: a [Client] exposes one service per area (System, Devices,
// Tunnels, Update, Backup, Auth), each returning clean public types. The
// low-level REST client generated from the daemon's OpenAPI document is an
// unexported implementation detail (see internal/rest) and never appears in
// this package's surface.
//
// # Usage
//
//	client, err := wardnet.New("http://127.0.0.1:7411", wardnet.WithToken(token))
//	if err != nil {
//		return err
//	}
//	status, err := client.System.Status(ctx)
//
// Every method takes a [context.Context] and returns a typed result or an
// error. Errors from the daemon are returned as [*APIError]; transport
// failures surface as the underlying error.
package wardnet
