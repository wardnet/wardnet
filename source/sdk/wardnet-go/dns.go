package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// DNSService configures the resolver and its filtering rules.
//
// Blocklists, allowlist entries, and custom rules are scoped to a filter
// profile, so every method that touches them takes a profile ID. Use
// [DNSService.Profiles] to discover the builtin ones ("Ad Blocking",
// "Parental Controls", "Malware & Phishing").
type DNSService struct{ c *Client }

// DNSResolutionMode is the resolver strategy: "forwarding" or "recursive".
type DNSResolutionMode string

// ForwarderSelectionMode is how the configured upstreams are used while
// forwarding: "failover", "fastest", or "single".
type ForwarderSelectionMode string

// UpstreamDNS is one configured upstream resolver.
type UpstreamDNS struct {
	// Name is the human-readable label (e.g. "Cloudflare").
	Name string `json:"name"`
	// Address is the IP or hostname to dial.
	Address string `json:"address"`
	// Protocol is the transport: "udp", "tcp", "tls", or "https".
	Protocol string `json:"protocol"`
	// Port is the upstream port, or 0 to use the protocol default.
	Port int32 `json:"port"`
	// TLSServerName is the SNI expected on the upstream's certificate. Set
	// only for the TLS and HTTPS protocols.
	TLSServerName string `json:"tls_server_name"`
}

// DNSConfig is the resolver's persisted configuration.
type DNSConfig struct {
	// Enabled reports whether the resolver should be running.
	Enabled bool `json:"enabled"`
	// ResolutionMode is the resolver strategy.
	ResolutionMode DNSResolutionMode `json:"resolution_mode"`
	// UpstreamServers are the configured forwarders.
	UpstreamServers []UpstreamDNS `json:"upstream_servers"`
	// ForwarderSelectionMode is how those forwarders are picked.
	ForwarderSelectionMode ForwarderSelectionMode `json:"forwarder_selection_mode"`
	// SingleUpstream is the chosen forwarder's address, set only when
	// ForwarderSelectionMode is "single".
	SingleUpstream string `json:"single_upstream"`
	// CacheSize is the maximum number of cached answers.
	CacheSize int32 `json:"cache_size"`
	// CacheTTLMinSecs / CacheTTLMaxSecs clamp upstream TTLs.
	CacheTTLMinSecs int32 `json:"cache_ttl_min_secs"`
	CacheTTLMaxSecs int32 `json:"cache_ttl_max_secs"`
	// DNSSECEnabled reports whether answers are validated.
	DNSSECEnabled bool `json:"dnssec_enabled"`
	// RebindingProtection drops private-range answers from public names.
	RebindingProtection bool `json:"rebinding_protection"`
	// RateLimitPerSecond bounds queries accepted from one client.
	RateLimitPerSecond int32 `json:"rate_limit_per_second"`
	// FilteringEnabled is the global filter kill switch: when false every
	// query passes regardless of per-device or per-profile state.
	FilteringEnabled bool `json:"dns_filtering_enabled"`
	// QueryLogEnabled reports whether queries are recorded.
	QueryLogEnabled bool `json:"query_log_enabled"`
	// QueryLogRetentionDays is how long those records are kept.
	QueryLogRetentionDays int32 `json:"query_log_retention_days"`
}

// UpstreamLatency is the background prober's view of one upstream.
type UpstreamLatency struct {
	// Address matches the configured [UpstreamDNS.Address].
	Address string `json:"address"`
	// Reachable reports whether the most recent probe got an answer.
	Reachable bool `json:"reachable"`
	// AvgLatencyMs is the rolling-average round trip, or nil before the
	// first successful probe.
	AvgLatencyMs *float64 `json:"avg_latency_ms"`
}

// DNSStatus is the resolver's live state.
type DNSStatus struct {
	// Enabled is the configured intent; Running is the observed reality.
	Enabled bool `json:"enabled"`
	Running bool `json:"running"`
	// CacheSize / CacheCapacity describe cache occupancy.
	CacheSize     int64 `json:"cache_size"`
	CacheCapacity int32 `json:"cache_capacity"`
	// CacheHitRate is the fraction of queries served from cache.
	CacheHitRate float64 `json:"cache_hit_rate"`
	// UpstreamLatencies is one entry per configured upstream, empty until
	// the prober has run.
	UpstreamLatencies []UpstreamLatency `json:"upstream_latencies"`
}

// FilterProfile is a named bundle of blocklists, allowlist entries, and
// custom rules.
type FilterProfile struct {
	// ID is the profile's UUID.
	ID string `json:"id"`
	// Name is the display name (e.g. "Ad Blocking").
	Name string `json:"name"`
	// Description is an optional annotation.
	Description string `json:"description"`
	// Builtin profiles cannot be renamed or deleted.
	Builtin bool `json:"builtin"`
	// CreatedAt / UpdatedAt are the profile's timestamps.
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// Blocklist is a URL-sourced domain list belonging to a filter profile.
type Blocklist struct {
	// ID is the blocklist's UUID.
	ID string `json:"id"`
	// ProfileID is the owning filter profile.
	ProfileID string `json:"profile_id"`
	// Name is the display name.
	Name string `json:"name"`
	// URL is where the list is downloaded from.
	URL string `json:"url"`
	// Enabled reports whether the list contributes to the filter.
	Enabled bool `json:"enabled"`
	// EntryCount is how many domains the last successful download yielded.
	EntryCount int64 `json:"entry_count"`
	// CronSchedule is the refresh schedule (e.g. "0 3 * * *").
	CronSchedule string `json:"cron_schedule"`
	// LastUpdated is the last successful refresh, if any.
	LastUpdated *time.Time `json:"last_updated"`
	// LastError and LastErrorAt describe the most recent failed refresh.
	// Both are cleared by a success.
	LastError   string     `json:"last_error"`
	LastErrorAt *time.Time `json:"last_error_at"`
	// ConsecutiveFailures drives the refresh runner's backoff.
	ConsecutiveFailures int32 `json:"consecutive_failures"`
	// CreatedAt / UpdatedAt are the blocklist's timestamps.
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// AllowlistEntry is a domain that overrides any blocklist match within its
// filter profile.
type AllowlistEntry struct {
	// ID is the entry's UUID.
	ID string `json:"id"`
	// ProfileID is the owning filter profile.
	ProfileID string `json:"profile_id"`
	// Domain is the allowed name.
	Domain string `json:"domain"`
	// Reason is an optional note explaining the exemption.
	Reason string `json:"reason"`
	// CreatedAt is when the entry was added.
	CreatedAt time.Time `json:"created_at"`
}

// FilterRule is a user-written AdGuard-syntax rule within a filter profile.
type FilterRule struct {
	// ID is the rule's UUID.
	ID string `json:"id"`
	// ProfileID is the owning filter profile.
	ProfileID string `json:"profile_id"`
	// Text is the rule itself (e.g. "||ads.example.com^").
	Text string `json:"rule_text"`
	// Enabled reports whether the rule is applied.
	Enabled bool `json:"enabled"`
	// Comment is an optional note.
	Comment string `json:"comment"`
	// CreatedAt / UpdatedAt are the rule's timestamps.
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// CreateBlocklistParams describes a blocklist to add.
type CreateBlocklistParams struct {
	// Name is the display name.
	Name string
	// URL is where the list is downloaded from.
	URL string
	// CronSchedule is the refresh schedule (e.g. "0 3 * * *").
	CronSchedule string
	// Enabled reports whether the list starts active.
	Enabled bool
}

// UpdateBlocklistParams is a partial blocklist update: nil fields are left
// unchanged.
type UpdateBlocklistParams struct {
	Name         *string
	URL          *string
	CronSchedule *string
	Enabled      *bool
}

// CreateRuleParams describes a custom filter rule to add.
type CreateRuleParams struct {
	// Text is the AdGuard-syntax rule.
	Text string
	// Comment is an optional note.
	Comment string
	// Enabled reports whether the rule starts active.
	Enabled bool
}

// UpdateRuleParams is a partial rule update: nil fields are left unchanged.
type UpdateRuleParams struct {
	Text    *string
	Comment *string
	Enabled *bool
}

// Config fetches the resolver configuration.
func (s *DNSService) Config(ctx context.Context) (*DNSConfig, error) {
	resp, err := s.c.rest.DnsGetConfigWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	cfg := dnsConfigFromREST(&resp.JSON200.Config)
	return &cfg, nil
}

// SetEnabled starts or stops the resolver and returns the resulting config.
func (s *DNSService) SetEnabled(ctx context.Context, enabled bool) (*DNSConfig, error) {
	resp, err := s.c.rest.DnsToggleWithResponse(ctx, rest.DnsToggleJSONRequestBody{Enabled: enabled})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	cfg := dnsConfigFromREST(&resp.JSON200.Config)
	return &cfg, nil
}

// Status reports the resolver's live state.
func (s *DNSService) Status(ctx context.Context) (*DNSStatus, error) {
	resp, err := s.c.rest.DnsStatusWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	m := resp.JSON200
	out := &DNSStatus{
		Enabled:           m.Enabled,
		Running:           m.Running,
		CacheSize:         m.CacheSize,
		CacheCapacity:     m.CacheCapacity,
		CacheHitRate:      m.CacheHitRate,
		UpstreamLatencies: make([]UpstreamLatency, 0, len(m.UpstreamLatencies)),
	}
	for _, l := range m.UpstreamLatencies {
		out.UpstreamLatencies = append(out.UpstreamLatencies, UpstreamLatency{
			Address:      l.Address,
			Reachable:    l.Reachable,
			AvgLatencyMs: l.AvgLatencyMs,
		})
	}
	return out, nil
}

// FlushCache empties the resolver cache and reports how many entries went.
func (s *DNSService) FlushCache(ctx context.Context) (int64, error) {
	resp, err := s.c.rest.FlushCacheWithResponse(ctx)
	if err != nil {
		return 0, err
	}
	if resp.JSON200 == nil {
		return 0, apiError(resp.HTTPResponse, resp.Body)
	}
	return resp.JSON200.EntriesCleared, nil
}

// Profiles returns every DNS filter profile.
func (s *DNSService) Profiles(ctx context.Context) ([]FilterProfile, error) {
	resp, err := s.c.rest.DnsFilterListProfilesWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]FilterProfile, 0, len(resp.JSON200.Profiles))
	for _, p := range resp.JSON200.Profiles {
		out = append(out, FilterProfile{
			ID:          p.Id.String(),
			Name:        p.Name,
			Description: deref(p.Description),
			Builtin:     p.Builtin,
			CreatedAt:   p.CreatedAt,
			UpdatedAt:   p.UpdatedAt,
		})
	}
	return out, nil
}

// Blocklists returns every blocklist in a profile.
func (s *DNSService) Blocklists(ctx context.Context, profileID string) ([]Blocklist, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ListBlocklistsWithResponse(ctx, pid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Blocklist, 0, len(resp.JSON200.Blocklists))
	for i := range resp.JSON200.Blocklists {
		out = append(out, blocklistFromREST(&resp.JSON200.Blocklists[i]))
	}
	return out, nil
}

// AddBlocklist creates a blocklist in a profile.
func (s *DNSService) AddBlocklist(ctx context.Context, profileID string, params CreateBlocklistParams) (*Blocklist, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.CreateBlocklistWithResponse(ctx, pid, rest.CreateBlocklistJSONRequestBody{
		Name:         params.Name,
		Url:          params.URL,
		CronSchedule: params.CronSchedule,
		Enabled:      params.Enabled,
	})
	if err != nil {
		return nil, err
	}
	if resp.JSON201 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	b := blocklistFromREST(&resp.JSON201.Blocklist)
	return &b, nil
}

// UpdateBlocklist applies a partial update to a blocklist.
func (s *DNSService) UpdateBlocklist(ctx context.Context, profileID, id string, params UpdateBlocklistParams) (*Blocklist, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	bid, err := parseUUID(id, "blocklist")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.UpdateBlocklistWithResponse(ctx, pid, bid, rest.UpdateBlocklistJSONRequestBody{
		Name:         params.Name,
		Url:          params.URL,
		CronSchedule: params.CronSchedule,
		Enabled:      params.Enabled,
	})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	b := blocklistFromREST(&resp.JSON200.Blocklist)
	return &b, nil
}

// RemoveBlocklist deletes a blocklist from a profile.
func (s *DNSService) RemoveBlocklist(ctx context.Context, profileID, id string) error {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return err
	}
	bid, err := parseUUID(id, "blocklist")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteBlocklistWithResponse(ctx, pid, bid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// RefreshBlocklist queues a re-download of a blocklist and returns the ID of
// the dispatched job. The download itself runs in the background.
func (s *DNSService) RefreshBlocklist(ctx context.Context, profileID, id string) (string, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return "", err
	}
	bid, err := parseUUID(id, "blocklist")
	if err != nil {
		return "", err
	}
	resp, err := s.c.rest.RefreshBlocklistWithResponse(ctx, pid, bid)
	if err != nil {
		return "", err
	}
	if resp.JSON202 == nil {
		return "", apiError(resp.HTTPResponse, resp.Body)
	}
	return resp.JSON202.JobId.String(), nil
}

// Allowlist returns every allowlist entry in a profile.
func (s *DNSService) Allowlist(ctx context.Context, profileID string) ([]AllowlistEntry, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ListAllowlistWithResponse(ctx, pid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]AllowlistEntry, 0, len(resp.JSON200.Entries))
	for i := range resp.JSON200.Entries {
		out = append(out, allowlistEntryFromREST(&resp.JSON200.Entries[i]))
	}
	return out, nil
}

// AddAllowlistEntry exempts a domain from the profile's blocklists. An empty
// reason is sent as no reason.
func (s *DNSService) AddAllowlistEntry(ctx context.Context, profileID, domain, reason string) (*AllowlistEntry, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	body := rest.CreateAllowlistEntryJSONRequestBody{Domain: domain}
	if reason != "" {
		body.Reason = ptr(reason)
	}
	resp, err := s.c.rest.CreateAllowlistEntryWithResponse(ctx, pid, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON201 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	e := allowlistEntryFromREST(&resp.JSON201.Entry)
	return &e, nil
}

// RemoveAllowlistEntry deletes an allowlist entry from a profile.
func (s *DNSService) RemoveAllowlistEntry(ctx context.Context, profileID, id string) error {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return err
	}
	eid, err := parseUUID(id, "allowlist entry")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteAllowlistEntryWithResponse(ctx, pid, eid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// Rules returns every custom filter rule in a profile.
func (s *DNSService) Rules(ctx context.Context, profileID string) ([]FilterRule, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ListCustomRulesWithResponse(ctx, pid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]FilterRule, 0, len(resp.JSON200.Rules))
	for i := range resp.JSON200.Rules {
		out = append(out, filterRuleFromREST(&resp.JSON200.Rules[i]))
	}
	return out, nil
}

// AddRule creates a custom filter rule in a profile.
func (s *DNSService) AddRule(ctx context.Context, profileID string, params CreateRuleParams) (*FilterRule, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	body := rest.CreateCustomRuleJSONRequestBody{
		RuleText: params.Text,
		Enabled:  params.Enabled,
	}
	if params.Comment != "" {
		body.Comment = ptr(params.Comment)
	}
	resp, err := s.c.rest.CreateCustomRuleWithResponse(ctx, pid, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON201 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	r := filterRuleFromREST(&resp.JSON201.Rule)
	return &r, nil
}

// UpdateRule applies a partial update to a custom filter rule.
func (s *DNSService) UpdateRule(ctx context.Context, profileID, id string, params UpdateRuleParams) (*FilterRule, error) {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return nil, err
	}
	rid, err := parseUUID(id, "rule")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.UpdateCustomRuleWithResponse(ctx, pid, rid, rest.UpdateCustomRuleJSONRequestBody{
		RuleText: params.Text,
		Comment:  params.Comment,
		Enabled:  params.Enabled,
	})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	r := filterRuleFromREST(&resp.JSON200.Rule)
	return &r, nil
}

// RemoveRule deletes a custom filter rule from a profile.
func (s *DNSService) RemoveRule(ctx context.Context, profileID, id string) error {
	pid, err := parseUUID(profileID, "profile")
	if err != nil {
		return err
	}
	rid, err := parseUUID(id, "rule")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteCustomRuleWithResponse(ctx, pid, rid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

func dnsConfigFromREST(c *rest.DnsConfig) DNSConfig {
	out := DNSConfig{
		Enabled:                c.Enabled,
		ResolutionMode:         DNSResolutionMode(c.ResolutionMode),
		ForwarderSelectionMode: ForwarderSelectionMode(c.ForwarderSelectionMode),
		SingleUpstream:         deref(c.SingleUpstream),
		CacheSize:              c.CacheSize,
		CacheTTLMinSecs:        c.CacheTtlMinSecs,
		CacheTTLMaxSecs:        c.CacheTtlMaxSecs,
		DNSSECEnabled:          c.DnssecEnabled,
		RebindingProtection:    c.RebindingProtection,
		RateLimitPerSecond:     c.RateLimitPerSecond,
		FilteringEnabled:       c.DnsFilteringEnabled,
		QueryLogEnabled:        c.QueryLogEnabled,
		QueryLogRetentionDays:  c.QueryLogRetentionDays,
		UpstreamServers:        make([]UpstreamDNS, 0, len(c.UpstreamServers)),
	}
	for _, u := range c.UpstreamServers {
		up := UpstreamDNS{
			Name:          u.Name,
			Address:       u.Address,
			Protocol:      string(u.Protocol),
			TLSServerName: deref(u.TlsServerName),
		}
		if u.Port != nil {
			up.Port = *u.Port
		}
		out.UpstreamServers = append(out.UpstreamServers, up)
	}
	return out
}

func blocklistFromREST(b *rest.Blocklist) Blocklist {
	return Blocklist{
		ID:                  b.Id.String(),
		ProfileID:           b.ProfileId.String(),
		Name:                b.Name,
		URL:                 b.Url,
		Enabled:             b.Enabled,
		EntryCount:          b.EntryCount,
		CronSchedule:        b.CronSchedule,
		LastUpdated:         b.LastUpdated,
		LastError:           deref(b.LastError),
		LastErrorAt:         b.LastErrorAt,
		ConsecutiveFailures: b.ConsecutiveFailures,
		CreatedAt:           b.CreatedAt,
		UpdatedAt:           b.UpdatedAt,
	}
}

func allowlistEntryFromREST(e *rest.AllowlistEntry) AllowlistEntry {
	return AllowlistEntry{
		ID:        e.Id.String(),
		ProfileID: e.ProfileId.String(),
		Domain:    e.Domain,
		Reason:    deref(e.Reason),
		CreatedAt: e.CreatedAt,
	}
}

func filterRuleFromREST(r *rest.CustomFilterRule) FilterRule {
	return FilterRule{
		ID:        r.Id.String(),
		ProfileID: r.ProfileId.String(),
		Text:      r.RuleText,
		Enabled:   r.Enabled,
		Comment:   deref(r.Comment),
		CreatedAt: r.CreatedAt,
		UpdatedAt: r.UpdatedAt,
	}
}
