// Package rest is the internal, generated REST client for the wardnet daemon
// API. It is generated from the daemon's OpenAPI document (docs/openapi.json)
// and MUST NOT be edited by hand or referenced outside this module: it lives
// under internal/ precisely so its machine-shaped types never leak into the
// public SDK surface. The hand-written packages in this module wrap it and
// translate to and from the public, domain-oriented types.
//
//go:generate go run github.com/oapi-codegen/oapi-codegen/v2/cmd/oapi-codegen@v2.8.0 -config cfg.yaml ../../../../../docs/openapi.json
package rest
