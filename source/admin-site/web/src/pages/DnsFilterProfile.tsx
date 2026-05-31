import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button } from "@wardnet/forge-web/button";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { Toggle } from "@wardnet/forge-web/toggle";
import { Pill } from "@wardnet/forge-web/pill";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { CronSchedulePicker } from "@/components/compound/CronSchedulePicker";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { BlocklistTable } from "@/components/compound/BlocklistTable";
import { AllowlistTable } from "@/components/compound/AllowlistTable";
import { FilterRuleTable } from "@/components/compound/FilterRuleTable";
import {
  useDnsFilterProfile,
  useUpdateDnsFilterProfile,
  useDeleteDnsFilterProfile,
  useBlocklists,
  useCreateBlocklist,
  useUpdateBlocklist,
  useDeleteBlocklist,
  useRefreshBlocklist,
  useAllowlist,
  useCreateAllowlistEntry,
  useDeleteAllowlistEntry,
  useFilterRules,
  useCreateFilterRule,
  useUpdateFilterRule,
  useDeleteFilterRule,
} from "@/hooks/useDnsFilter";
import type { Blocklist } from "@wardnet/js";

const DEFAULT_SCHEDULE = "0 3 * * *";

/** Profile detail page: identity card + three sub-section cards. */
export default function DnsFilterProfile() {
  const { id = "" } = useParams<{ id: string }>();
  const navigate = useNavigate();

  const { data, isLoading, isError } = useDnsFilterProfile(id);
  const profile = data?.profile;

  if (isLoading) {
    return (
      <div className="p-6">
        <p className="text-sm text-ink-3">Loading…</p>
      </div>
    );
  }

  if (isError || !profile) {
    return (
      <div className="flex flex-col gap-2 p-6">
        <h1 className="text-xl font-semibold">Profile not found</h1>
        <Link to="/dns/filter" className="text-sm text-accent underline">
          Back to DNS Filtering
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-4 md:p-6">
      <DetailPageHeader
        parentLabel="DNS Filtering"
        parentTo="/dns/filter"
        itemLabel={profile.name}
        status={profile.builtin ? <Pill variant="ghost">Builtin</Pill> : undefined}
      />

      <ProfileIdentityCard
        profileId={profile.id}
        name={profile.name}
        description={profile.description}
        builtin={profile.builtin}
        onDeleted={() => void navigate("/dns/filter")}
      />

      <BlocklistsCard profileId={profile.id} />
      <AllowlistCard profileId={profile.id} />
      <CustomRulesCard profileId={profile.id} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Identity card — rename, delete (when not builtin)
// ---------------------------------------------------------------------------

interface IdentityCardProps {
  profileId: string;
  name: string;
  description: string | null;
  builtin: boolean;
  onDeleted: () => void;
}

function ProfileIdentityCard({
  profileId,
  name,
  description,
  builtin,
  onDeleted,
}: IdentityCardProps) {
  const update = useUpdateDnsFilterProfile();
  const del = useDeleteDnsFilterProfile();

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const [draftDesc, setDraftDesc] = useState(description ?? "");
  const [confirmDelete, setConfirmDelete] = useState(false);

  function startEdit() {
    setDraft(name);
    setDraftDesc(description ?? "");
    update.reset();
    setEditing(true);
  }

  async function handleSave() {
    const nameChanged = draft.trim() !== name;
    const descChanged = draftDesc.trim() !== (description ?? "");
    await update.mutateAsync({
      id: profileId,
      body: {
        name: nameChanged ? draft.trim() : undefined,
        // empty string → null (clear); unchanged → undefined (omit)
        description: descChanged ? draftDesc.trim() || null : undefined,
      },
    });
    setEditing(false);
  }

  async function handleDelete() {
    await del.mutateAsync(profileId);
    onDeleted();
  }

  const saveDisabled =
    update.isPending ||
    draft.trim() === "" ||
    (draft.trim() === name && draftDesc.trim() === (description ?? ""));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Identity</CardTitle>
        {!editing && !builtin && (
          <CardAction>
            <Button variant="outline" size="sm" onClick={startEdit}>
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-5">
            <Field label="Name" htmlFor="profile-name">
              <Input id="profile-name" value={draft} onChange={(e) => setDraft(e.target.value)} />
            </Field>
            <Field label="Description (optional)" htmlFor="profile-desc">
              <Input
                id="profile-desc"
                value={draftDesc}
                onChange={(e) => setDraftDesc(e.target.value)}
                placeholder="e.g. Strict — for kids' devices"
              />
            </Field>
            {update.isError && (
              <ApiErrorAlert error={update.error} fallback="Failed to update profile" />
            )}
          </CardContent>
          <CardFooter className="justify-between gap-2">
            <Button
              variant="ghost"
              className="text-danger hover:text-danger"
              onClick={() => setConfirmDelete(true)}
              disabled={builtin || update.isPending || del.isPending}
            >
              Delete profile
            </Button>
            <div className="flex gap-2">
              <Button variant="ghost" onClick={() => setEditing(false)} disabled={update.isPending}>
                Cancel
              </Button>
              <Button onClick={handleSave} disabled={saveDisabled}>
                {update.isPending ? "Saving…" : "Save"}
              </Button>
            </div>
          </CardFooter>
        </>
      ) : (
        <CardContent className="grid grid-cols-1 gap-x-6 gap-y-4 lg:grid-cols-2">
          <div className="flex flex-col gap-0.5">
            <span className="text-xs uppercase tracking-wide text-ink-3">Name</span>
            <span className="text-sm">{name}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs uppercase tracking-wide text-ink-3">Type</span>
            <span className="text-sm">{builtin ? "Builtin" : "Custom"}</span>
          </div>
          {(description !== null || builtin) && (
            <div className="flex flex-col gap-0.5 lg:col-span-2">
              <span className="text-xs uppercase tracking-wide text-ink-3">Description</span>
              <span className="text-sm text-ink-2">{description ?? "—"}</span>
            </div>
          )}
        </CardContent>
      )}

      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title="Delete profile"
        description={`Delete "${name}"? Devices assigned to this profile will fall back to the default profile.`}
        confirmLabel="Delete"
        onConfirm={handleDelete}
      />
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Blocklists section
// ---------------------------------------------------------------------------

interface SubSectionProps {
  profileId: string;
}

function BlocklistsCard({ profileId }: SubSectionProps) {
  const { data } = useBlocklists(profileId);
  const create = useCreateBlocklist(profileId);
  const update = useUpdateBlocklist(profileId);
  const remove = useDeleteBlocklist(profileId);
  const refresh = useRefreshBlocklist(profileId);

  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<Blocklist | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const blocklists = data?.blocklists ?? [];
  const toDelete = blocklists.find((b) => b.id === deleteId);

  function startAdd() {
    setEditing(null);
    setAdding(true);
  }
  function startEdit(b: Blocklist) {
    setAdding(false);
    setEditing(b);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Blocklists</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {adding && (
          <BlocklistForm
            mode="create"
            onCancel={() => setAdding(false)}
            onSubmit={async (body) => {
              await create.mutateAsync(body);
              setAdding(false);
            }}
            isSaving={create.isPending}
            error={create.isError ? create.error : null}
          />
        )}
        {editing && (
          <BlocklistForm
            mode="edit"
            initial={editing}
            onCancel={() => setEditing(null)}
            onSubmit={async (body) => {
              await update.mutateAsync({ id: editing.id, body });
              setEditing(null);
            }}
            isSaving={update.isPending}
            error={update.isError ? update.error : null}
          />
        )}

        <BlocklistTable
          blocklists={blocklists}
          refreshingId={refresh.isPending ? (refresh.variables ?? null) : null}
          onRefresh={(blId) => refresh.mutate(blId)}
          onToggle={(b) => update.mutate({ id: b.id, body: { enabled: !b.enabled } })}
          onEdit={startEdit}
          onDelete={setDeleteId}
          onAdd={startAdd}
        />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(o) => !o && setDeleteId(null)}
        title="Delete blocklist"
        description={`Delete "${toDelete?.name ?? "this blocklist"}"? It will no longer be downloaded or used for filtering.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) remove.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

interface BlocklistFormProps {
  mode: "create" | "edit";
  initial?: Blocklist;
  onSubmit: (body: {
    name: string;
    url: string;
    cron_schedule: string;
    enabled: boolean;
  }) => Promise<void> | void;
  onCancel: () => void;
  isSaving: boolean;
  error: unknown;
}

function BlocklistForm({ mode, initial, onSubmit, onCancel, isSaving, error }: BlocklistFormProps) {
  const [name, setName] = useState(initial?.name ?? "");
  const [url, setUrl] = useState(initial?.url ?? "");
  const [schedule, setSchedule] = useState(initial?.cron_schedule ?? DEFAULT_SCHEDULE);
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);

  const canSave = name.trim() !== "" && url.trim() !== "";

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>{mode === "edit" ? "Edit blocklist" : "Add blocklist"}</CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 lg:grid-cols-2">
        <Field label="Name" htmlFor="bl-name">
          <Input
            id="bl-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Steven Black Hosts"
          />
        </Field>
        <Field
          label="URL"
          htmlFor="bl-url"
          help="Hosts file, ABP/AdGuard list, or domain-per-line format."
        >
          <Input
            id="bl-url"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts"
          />
        </Field>
        <CronSchedulePicker label="Update schedule" value={schedule} onChange={setSchedule} />
        <Field label={mode === "edit" ? "Enabled" : "Enable immediately"} htmlFor="bl-enabled">
          <div className="flex h-9 items-center justify-between">
            <span className="text-sm text-ink-3">
              {enabled ? "Active in this profile" : "Saved but not applied"}
            </span>
            <Toggle id="bl-enabled" checked={enabled} onCheckedChange={setEnabled} />
          </div>
        </Field>
        {error != null && (
          <div className="lg:col-span-2">
            <ApiErrorAlert
              error={error}
              fallback={mode === "edit" ? "Failed to update blocklist" : "Failed to add blocklist"}
            />
          </div>
        )}
      </CardContent>
      <CardFooter className="justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} disabled={isSaving}>
          Cancel
        </Button>
        <Button
          onClick={() => onSubmit({ name, url, cron_schedule: schedule, enabled })}
          disabled={isSaving || !canSave}
        >
          {isSaving
            ? mode === "edit"
              ? "Saving…"
              : "Adding…"
            : mode === "edit"
              ? "Save changes"
              : "Add blocklist"}
        </Button>
      </CardFooter>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Allowlist section
// ---------------------------------------------------------------------------

function AllowlistCard({ profileId }: SubSectionProps) {
  const { data } = useAllowlist(profileId);
  const create = useCreateAllowlistEntry(profileId);
  const remove = useDeleteAllowlistEntry(profileId);

  const [adding, setAdding] = useState(false);
  const [domain, setDomain] = useState("");
  const [reason, setReason] = useState("");
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const entries = data?.entries ?? [];
  const toDelete = entries.find((e) => e.id === deleteId);

  function startAdd() {
    setDomain("");
    setReason("");
    create.reset();
    setAdding(true);
  }

  async function handleSave() {
    await create.mutateAsync({ domain: domain.trim(), reason: reason.trim() || undefined });
    setAdding(false);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Allowlist</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {adding && (
          <Card className="border-dashed">
            <CardHeader>
              <CardTitle>Allow domain</CardTitle>
            </CardHeader>
            <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 lg:grid-cols-2">
              <Field
                label="Domain"
                htmlFor="al-domain"
                help="This domain will never be blocked under this profile, even if it appears in a blocklist."
              >
                <Input
                  id="al-domain"
                  value={domain}
                  onChange={(e) => setDomain(e.target.value)}
                  placeholder="example.com"
                />
              </Field>
              <Field label="Reason (optional)" htmlFor="al-reason">
                <Input
                  id="al-reason"
                  value={reason}
                  onChange={(e) => setReason(e.target.value)}
                  placeholder="Required for work VPN"
                />
              </Field>
              {create.isError && (
                <div className="lg:col-span-2">
                  <ApiErrorAlert error={create.error} fallback="Failed to add allowlist entry" />
                </div>
              )}
            </CardContent>
            <CardFooter className="justify-end gap-2">
              <Button variant="ghost" onClick={() => setAdding(false)} disabled={create.isPending}>
                Cancel
              </Button>
              <Button onClick={handleSave} disabled={create.isPending || domain.trim() === ""}>
                {create.isPending ? "Adding…" : "Allow domain"}
              </Button>
            </CardFooter>
          </Card>
        )}

        <AllowlistTable entries={entries} onDelete={setDeleteId} onAdd={startAdd} />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(o) => !o && setDeleteId(null)}
        title="Remove allowlist entry"
        description={`Remove "${toDelete?.domain ?? "this domain"}" from the allowlist?`}
        confirmLabel="Remove"
        onConfirm={() => {
          if (deleteId) remove.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Custom rules section
// ---------------------------------------------------------------------------

function CustomRulesCard({ profileId }: SubSectionProps) {
  const { data } = useFilterRules(profileId);
  const create = useCreateFilterRule(profileId);
  const update = useUpdateFilterRule(profileId);
  const remove = useDeleteFilterRule(profileId);

  const [adding, setAdding] = useState(false);
  const [ruleText, setRuleText] = useState("");
  const [comment, setComment] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const rules = data?.rules ?? [];
  const toDelete = rules.find((r) => r.id === deleteId);

  function startAdd() {
    setRuleText("");
    setComment("");
    setEnabled(true);
    create.reset();
    setAdding(true);
  }

  async function handleSave() {
    await create.mutateAsync({
      rule_text: ruleText.trim(),
      comment: comment.trim() || undefined,
      enabled,
    });
    setAdding(false);
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Custom rules</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {adding && (
          <Card className="border-dashed">
            <CardHeader>
              <CardTitle>Add filter rule</CardTitle>
            </CardHeader>
            <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 lg:grid-cols-2">
              <Field
                label="Rule"
                htmlFor="fr-text"
                help="AdGuard / ABP syntax."
                className="lg:col-span-2"
              >
                <Input
                  id="fr-text"
                  value={ruleText}
                  onChange={(e) => setRuleText(e.target.value)}
                  placeholder="||example.com^"
                  className="font-mono"
                />
              </Field>
              <Field label="Comment (optional)" htmlFor="fr-comment">
                <Input
                  id="fr-comment"
                  value={comment}
                  onChange={(e) => setComment(e.target.value)}
                />
              </Field>
              <Field label="Enable immediately" htmlFor="fr-enabled">
                <div className="flex h-9 items-center justify-between">
                  <span className="text-sm text-ink-3">
                    {enabled ? "Applied to traffic" : "Saved but inactive"}
                  </span>
                  <Toggle id="fr-enabled" checked={enabled} onCheckedChange={setEnabled} />
                </div>
              </Field>
              {create.isError && (
                <div className="lg:col-span-2">
                  <ApiErrorAlert error={create.error} fallback="Failed to add filter rule" />
                </div>
              )}
            </CardContent>
            <CardFooter className="justify-end gap-2">
              <Button variant="ghost" onClick={() => setAdding(false)} disabled={create.isPending}>
                Cancel
              </Button>
              <Button onClick={handleSave} disabled={create.isPending || ruleText.trim() === ""}>
                {create.isPending ? "Adding…" : "Add rule"}
              </Button>
            </CardFooter>
          </Card>
        )}

        <FilterRuleTable
          rules={rules}
          onToggle={(rId, en) => update.mutate({ id: rId, body: { enabled: en } })}
          onDelete={setDeleteId}
          onAdd={startAdd}
        />
      </CardContent>

      <ConfirmDialog
        open={!!deleteId}
        onOpenChange={(o) => !o && setDeleteId(null)}
        title="Delete filter rule"
        description={`Delete rule "${toDelete?.rule_text ?? ""}"?`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteId) remove.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </Card>
  );
}
