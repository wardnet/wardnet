package wardnet

import (
	"context"
	"net/http"
	"strings"
	"time"

	"wardnet.network/go/internal/rest"
)

// DefaultBaseURL is the daemon's plain-HTTP LAN admin endpoint, used when
// [New] is called with an empty base URL.
const DefaultBaseURL = "http://127.0.0.1:7411"

// defaultTimeout bounds each request made by the default HTTP client, so a
// caller who forgot a context deadline can't hang forever on a stalled daemon.
const defaultTimeout = 120 * time.Second

// Client talks to one wardnet daemon. Construct it with [New]; the per-area
// services hang off it.
type Client struct {
	rest *rest.ClientWithResponses

	// baseURL, httpClient, token, and userAgent are retained for the log
	// stream, which upgrades to a WebSocket and so cannot go through the
	// generated REST client.
	baseURL    string
	httpClient *http.Client
	token      string
	userAgent  string

	// System reports daemon health and runtime status.
	System *SystemService
	// Devices lists discovered devices and manages their routing.
	Devices *DevicesService
	// Tunnels manages WireGuard tunnels.
	Tunnels *TunnelsService
	// DNS configures the resolver and its filtering rules.
	DNS *DNSService
	// DHCP configures the DHCP server, its leases, and its reservations.
	DHCP *DHCPService
	// Logs downloads and streams daemon logs.
	Logs *LogsService
	// Update drives the auto-update subsystem.
	Update *UpdateService
	// Backup exports and restores encrypted configuration bundles.
	Backup *BackupService
	// Auth exchanges credentials for a session token.
	Auth *AuthService
	// Anomalies reads the daemon's open and resolved anomalies.
	Anomalies *AnomaliesService

	// Users manages the household user directory, enrolment invitations, and
	// federated sign-in configuration (ADR-0031).
	Users *UsersService
}

type options struct {
	httpClient *http.Client
	token      string
	userAgent  string
}

// Option configures a [Client].
type Option func(*options)

// WithToken sets the bearer credential (a session token or API key) sent as
// Authorization on every request.
func WithToken(token string) Option {
	return func(o *options) { o.token = token }
}

// WithHTTPClient supplies the HTTP client used for requests. Use it to set
// timeouts, proxies, or a custom transport. A nil client is ignored (the
// default client is used), so callers can pass through an optional client
// without tripping a typed-nil panic.
func WithHTTPClient(hc *http.Client) Option {
	return func(o *options) {
		if hc != nil {
			o.httpClient = hc
		}
	}
}

// WithUserAgent overrides the User-Agent header sent on every request.
func WithUserAgent(ua string) Option {
	return func(o *options) { o.userAgent = ua }
}

// New builds a [Client] for the daemon at baseURL (defaulting to
// [DefaultBaseURL] when empty).
func New(baseURL string, opts ...Option) (*Client, error) {
	o := options{userAgent: "wardnet-go"}
	for _, opt := range opts {
		opt(&o)
	}
	if baseURL == "" {
		baseURL = DefaultBaseURL
	}
	baseURL = strings.TrimRight(baseURL, "/")

	editor := func(_ context.Context, req *http.Request) error {
		if o.token != "" {
			req.Header.Set("Authorization", "Bearer "+o.token)
		}
		if o.userAgent != "" {
			req.Header.Set("User-Agent", o.userAgent)
		}
		return nil
	}

	hc := o.httpClient
	if hc == nil {
		// A default per-request timeout so a hung daemon doesn't block a
		// caller who forgot a context deadline. Callers with long-running
		// operations (large backup export/import) should supply their own
		// client via WithHTTPClient.
		hc = &http.Client{Timeout: defaultTimeout}
	}
	clientOpts := []rest.ClientOption{
		rest.WithRequestEditorFn(editor),
		rest.WithHTTPClient(hc),
	}

	rc, err := rest.NewClientWithResponses(baseURL, clientOpts...)
	if err != nil {
		return nil, err
	}

	c := &Client{
		rest:       rc,
		baseURL:    baseURL,
		httpClient: hc,
		token:      o.token,
		userAgent:  o.userAgent,
	}
	c.System = &SystemService{c: c}
	c.Devices = &DevicesService{c: c}
	c.Tunnels = &TunnelsService{c: c}
	c.DNS = &DNSService{c: c}
	c.DHCP = &DHCPService{c: c}
	c.Logs = &LogsService{c: c}
	c.Update = &UpdateService{c: c}
	c.Backup = &BackupService{c: c}
	c.Auth = &AuthService{c: c}
	c.Anomalies = &AnomaliesService{c: c}
	c.Users = &UsersService{c: c}
	return c, nil
}

// deref returns the pointed-to string, or "" when p is nil. Optional strings
// on the wire (name, hostname, …) map to empty strings in the public API.
func deref(p *string) string {
	if p == nil {
		return ""
	}
	return *p
}

// ptr returns a pointer to v. Used to build partial request bodies.
func ptr[T any](v T) *T { return &v }
