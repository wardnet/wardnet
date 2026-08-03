package wardnet

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"time"

	"wardnet.network/go/internal/rest"
)

// BackupService exports and restores encrypted configuration bundles.
type BackupService struct{ c *Client }

// BackupStateKind is the backup subsystem's coarse state.
type BackupStateKind string

const (
	BackupIdle      BackupStateKind = "idle"
	BackupExporting BackupStateKind = "exporting"
	BackupImporting BackupStateKind = "importing"
	BackupFailed    BackupStateKind = "failed"
)

// BackupStatus is the backup subsystem's current state. Phase is set while
// [BackupImporting]; Reason is set on [BackupFailed].
type BackupStatus struct {
	State  BackupStateKind
	Phase  string
	Reason string
}

// MarshalJSON encodes the status in the daemon's internally-tagged form.
func (b BackupStatus) MarshalJSON() ([]byte, error) {
	switch b.State {
	case BackupImporting:
		return json.Marshal(struct {
			State string `json:"state"`
			Phase string `json:"phase"`
		}{"importing", b.Phase})
	case BackupFailed:
		return json.Marshal(struct {
			State  string `json:"state"`
			Reason string `json:"reason"`
		}{"failed", b.Reason})
	default:
		return json.Marshal(struct {
			State string `json:"state"`
		}{string(b.State)})
	}
}

// UnmarshalJSON decodes the daemon's internally-tagged backup-status form.
func (b *BackupStatus) UnmarshalJSON(data []byte) error {
	var probe struct {
		State  string `json:"state"`
		Phase  string `json:"phase"`
		Reason string `json:"reason"`
	}
	if err := json.Unmarshal(data, &probe); err != nil {
		return err
	}
	*b = BackupStatus{
		State:  BackupStateKind(probe.State),
		Phase:  probe.Phase,
		Reason: probe.Reason,
	}
	return nil
}

// Snapshot is a `.bak-<timestamp>` copy retained from a prior restore.
type Snapshot struct {
	// Path is the snapshot's location on disk.
	Path string `json:"path"`
	// Kind is what was snapshotted: "database", "config", or "keys".
	Kind string `json:"kind"`
	// CreatedAt is when the snapshot was taken.
	CreatedAt time.Time `json:"created_at"`
	// SizeBytes is the snapshot size on disk.
	SizeBytes int64 `json:"size_bytes"`
}

// BundleManifest describes an exported bundle.
type BundleManifest struct {
	// WardnetVersion is the daemon version that produced the bundle.
	WardnetVersion string `json:"wardnet_version"`
	// SchemaVersion is the highest applied migration at export time.
	SchemaVersion int64 `json:"schema_version"`
	// CreatedAt is when the bundle was created.
	CreatedAt time.Time `json:"created_at"`
	// HostID identifies the source host.
	HostID string `json:"host_id"`
	// BundleFormatVersion is the bundle layout version.
	BundleFormatVersion int32 `json:"bundle_format_version"`
	// KeyCount is how many WireGuard key files the bundle carries.
	KeyCount int32 `json:"key_count"`
}

// RestorePreview is the result of previewing a bundle before applying it.
type RestorePreview struct {
	// Manifest is the decrypted, validated bundle manifest.
	Manifest BundleManifest `json:"manifest"`
	// Compatible reports whether the bundle can be applied to this daemon.
	Compatible bool `json:"compatible"`
	// IncompatibilityReason explains an incompatible bundle, else "".
	IncompatibilityReason string `json:"incompatibility_reason"`
	// FilesToReplace lists the files the apply step will overwrite.
	FilesToReplace []string `json:"files_to_replace"`
	// PreviewToken authorizes a matching ApplyImport call.
	PreviewToken string `json:"preview_token"`
}

// RestoreResult is the outcome of applying a bundle.
type RestoreResult struct {
	// Manifest is the applied bundle's manifest.
	Manifest BundleManifest `json:"manifest"`
	// Snapshots are the `.bak-<timestamp>` copies taken of replaced files.
	Snapshots []Snapshot `json:"snapshots"`
}

// Status returns the current backup subsystem status.
func (s *BackupService) Status(ctx context.Context) (*BackupStatus, error) {
	resp, err := s.c.rest.BackupStatusWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	st, err := backupStatusFromREST(resp.JSON200.Status)
	if err != nil {
		return nil, err
	}
	return &st, nil
}

// Snapshots lists the `.bak-<timestamp>` snapshots retained from prior restores.
func (s *BackupService) Snapshots(ctx context.Context) ([]Snapshot, error) {
	resp, err := s.c.rest.ListSnapshotsWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return snapshotsFromREST(resp.JSON200.Snapshots), nil
}

// Export produces an encrypted bundle protected by passphrase and returns its
// bytes.
func (s *BackupService) Export(ctx context.Context, passphrase string) ([]byte, error) {
	resp, err := s.c.rest.ExportWithResponse(ctx, rest.ExportJSONRequestBody{Passphrase: passphrase})
	if err != nil {
		return nil, err
	}
	if statusOf(resp.HTTPResponse) != 200 {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return resp.Body, nil
}

// PreviewImport decrypts and validates a bundle without changing any state,
// returning what an apply would do.
func (s *BackupService) PreviewImport(ctx context.Context, bundle []byte, passphrase string) (*RestorePreview, error) {
	body, contentType := multipartBundle(bundle, passphrase)
	resp, err := s.c.rest.PreviewImportWithBodyWithResponse(ctx, contentType, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	m := resp.JSON200
	return &RestorePreview{
		Manifest:              manifestFromREST(m.Manifest),
		Compatible:            m.Compatible,
		IncompatibilityReason: deref(m.IncompatibilityReason),
		FilesToReplace:        m.FilesToReplace,
		PreviewToken:          m.PreviewToken,
	}, nil
}

// ApplyImport commits a previously-previewed restore. A daemon restart is
// required afterward for the new state to take effect.
func (s *BackupService) ApplyImport(ctx context.Context, previewToken string) (*RestoreResult, error) {
	resp, err := s.c.rest.ApplyImportWithResponse(ctx, rest.ApplyImportJSONRequestBody{PreviewToken: previewToken})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &RestoreResult{
		Manifest:  manifestFromREST(resp.JSON200.Manifest),
		Snapshots: snapshotsFromREST(resp.JSON200.Snapshots),
	}, nil
}

// Import restores a bundle in one call: it previews, refuses an incompatible
// bundle, then applies. A daemon restart is required afterward.
func (s *BackupService) Import(ctx context.Context, bundle []byte, passphrase string) (*RestoreResult, error) {
	preview, err := s.PreviewImport(ctx, bundle, passphrase)
	if err != nil {
		return nil, err
	}
	if !preview.Compatible {
		reason := preview.IncompatibilityReason
		if reason == "" {
			reason = "incompatible bundle"
		}
		return nil, fmt.Errorf("wardnet: bundle cannot be restored: %s", reason)
	}
	return s.ApplyImport(ctx, preview.PreviewToken)
}

// multipartBundle streams the multipart body PreviewImport expects — a
// `bundle` file part plus a `passphrase` text field — through an io.Pipe, so
// the bundle is not copied into a second full in-memory buffer.
func multipartBundle(bundle []byte, passphrase string) (io.Reader, string) {
	pr, pw := io.Pipe()
	mw := multipart.NewWriter(pw)
	// The content type carries the boundary, fixed at writer creation; read it
	// before the goroutine touches the writer.
	contentType := mw.FormDataContentType()
	go func() {
		err := writeBundleParts(mw, bundle, passphrase)
		if cerr := mw.Close(); err == nil {
			err = cerr
		}
		_ = pw.CloseWithError(err)
	}()
	return pr, contentType
}

func writeBundleParts(mw *multipart.Writer, bundle []byte, passphrase string) error {
	fw, err := mw.CreateFormFile("bundle", "bundle.wardnet.age")
	if err != nil {
		return err
	}
	if _, err := fw.Write(bundle); err != nil {
		return err
	}
	return mw.WriteField("passphrase", passphrase)
}

func backupStatusFromREST(rs rest.BackupStatus) (BackupStatus, error) {
	return bridgeJSON[BackupStatus](rs)
}

func manifestFromREST(m rest.BundleManifest) BundleManifest {
	return BundleManifest{
		WardnetVersion:      m.WardnetVersion,
		SchemaVersion:       m.SchemaVersion,
		CreatedAt:           m.CreatedAt,
		HostID:              m.HostId,
		BundleFormatVersion: m.BundleFormatVersion,
		KeyCount:            m.KeyCount,
	}
}

func snapshotsFromREST(in []rest.LocalSnapshot) []Snapshot {
	out := make([]Snapshot, 0, len(in))
	for i := range in {
		out = append(out, Snapshot{
			Path:      in[i].Path,
			Kind:      string(in[i].Kind),
			CreatedAt: in[i].CreatedAt,
			SizeBytes: in[i].SizeBytes,
		})
	}
	return out
}
