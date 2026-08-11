module github.com/wardnet/wardnet/source/wctl

go 1.26.0

toolchain go1.26.5

require (
	github.com/BurntSushi/toml v1.6.0
	github.com/coder/websocket v1.8.15
	github.com/spf13/cobra v1.10.2
	golang.org/x/term v0.45.0
	wardnet.network/go v0.0.0
)

require (
	github.com/apapsch/go-jsonmerge/v2 v2.0.0 // indirect
	github.com/google/uuid v1.6.0 // indirect
	github.com/inconshreveable/mousetrap v1.1.0 // indirect
	github.com/oapi-codegen/runtime v1.6.0 // indirect
	github.com/spf13/pflag v1.0.9 // indirect
	golang.org/x/sys v0.47.0 // indirect
)

replace wardnet.network/go => ../sdk/wardnet-go
