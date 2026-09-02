import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import type { DataTableColumnDef } from "@/components/core/ui/data-table";
import { Split } from "lucide-react";
import { Button } from "@wardnet/web";
import { Card, CardContent } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import {
  Modal,
  ModalContent,
  ModalBody,
  ModalFooter,
  ModalTitleBlock,
} from "@wardnet/web";
import {
  useRoutingProfiles,
  useCreateRoutingProfile,
  useUpdateRoutingProfile,
  useDeleteRoutingProfile,
} from "@wardnet/web";
import type { RoutingProfile } from "@wardnet/js";
import { DataTable } from "@/components/core/ui/data-table";
import { PageHeader } from "@/components/compound/PageHeader";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";

/** Per-row badge showing how many domain rules a profile carries. The count
 *  comes from the profile list response (`rule_count`), so no per-row query is
 *  needed. Omitted while zero so a fresh profile reads uncluttered. */
function RuleCountBadge({ count }: { count: number }) {
  if (count === 0) return null;
  return (
    <Pill variant="ghost">
      {count.toLocaleString()} {count === 1 ? "rule" : "rules"}
    </Pill>
  );
}

/**
 * Routing profiles list — bundles of per-domain routing rules ("`*.netflix.com`
 * → UK tunnel") assigned to devices in priority order (issue #241). Create,
 * rename and delete live here; a row opens the profile's rules editor.
 */
export default function RoutingProfiles() {
  const navigate = useNavigate();
  const { data, isLoading } = useRoutingProfiles();
  const create = useCreateRoutingProfile();
  const rename = useUpdateRoutingProfile();
  const remove = useDeleteRoutingProfile();

  const profiles = data?.profiles ?? [];
  const hasProfiles = profiles.length > 0;

  // `null` closed; `{}` = create; `{profile}` = rename an existing one.
  const [nameModal, setNameModal] = useState<{
    profile?: RoutingProfile;
  } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<RoutingProfile | null>(null);

  function open(profile: RoutingProfile) {
    void navigate(`/routing/${profile.id}`);
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    await remove.mutateAsync(deleteTarget.id);
    setDeleteTarget(null);
  }

  const columns = useMemo<DataTableColumnDef<RoutingProfile>[]>(
    () => [
      {
        id: "profile",
        header: "Profile",
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-3">
            <div className="grid size-10 shrink-0 place-items-center rounded-lg bg-sunken">
              <Split className="size-5 text-ink-3" aria-hidden />
            </div>
            <Text
              weight="medium"
              className="min-w-0 truncate"
              title={row.original.name}
            >
              {row.original.name}
            </Text>
          </div>
        ),
      },
      {
        id: "counts",
        header: "",
        meta: { className: "hidden text-right md:table-cell" },
        cell: ({ row }) => (
          <div className="flex justify-end">
            <RuleCountBadge count={row.original.rule_count} />
          </div>
        ),
      },
      {
        id: "actions",
        header: "",
        meta: { className: "w-40 text-right" },
        cell: ({ row }) => (
          // Actions are not row navigation — stop propagation so the buttons
          // don't also open the rules editor.
          <div
            className="flex justify-end gap-2"
            onClick={(e) => e.stopPropagation()}
          >
            <Button
              variant="outline"
              size="sm"
              onClick={() => setNameModal({ profile: row.original })}
              data-testid="routing-profile-rename"
            >
              Rename
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setDeleteTarget(row.original)}
              data-testid="routing-profile-delete"
            >
              Delete
            </Button>
          </div>
        ),
      },
    ],
    [],
  );

  return (
    <div className="col gap-20">
      <PageHeader
        title="Routing"
        description="Route traffic to specific domains through a chosen tunnel — or carve it out to go direct. Group rules into profiles and assign them to devices in priority order."
        actions={
          hasProfiles ? (
            <Button
              onClick={() => setNameModal({})}
              data-testid="routing-add-profile"
            >
              New profile
            </Button>
          ) : undefined
        }
      />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">
            Loading profiles…
          </CardContent>
        </Card>
      )}

      {!isLoading && !hasProfiles && (
        <EmptyStatePlaceholder
          message="No routing profiles"
          hint="A routing profile bundles per-domain rules. Assign profiles to devices on the device detail page."
          actionLabel="New profile"
          onAction={() => setNameModal({})}
          actionTestId="routing-empty-add"
        />
      )}

      {!isLoading && hasProfiles && (
        <DataTable columns={columns} data={profiles} onRowClick={open} />
      )}

      {nameModal && (
        <ProfileNameModal
          profile={nameModal.profile}
          onClose={() => setNameModal(null)}
          onSubmit={async (name) => {
            if (nameModal.profile) {
              await rename.mutateAsync({
                id: nameModal.profile.id,
                body: { name },
              });
            } else {
              await create.mutateAsync({ name });
            }
            setNameModal(null);
          }}
          pending={create.isPending || rename.isPending}
          error={create.error ?? rename.error}
        />
      )}

      <ConfirmDialog
        open={deleteTarget !== null}
        onOpenChange={(next) => !next && setDeleteTarget(null)}
        title="Delete routing profile"
        description={
          deleteTarget
            ? `Delete "${deleteTarget.name}"? Its rules and every device assignment are removed.`
            : ""
        }
        confirmLabel="Delete"
        onConfirm={handleDelete}
      />
    </div>
  );
}

interface ProfileNameModalProps {
  profile?: RoutingProfile;
  onClose: () => void;
  onSubmit: (name: string) => void | Promise<void>;
  pending: boolean;
  error: unknown;
}

/** Create-or-rename modal — a single name field. */
function ProfileNameModal({
  profile,
  onClose,
  onSubmit,
  pending,
  error,
}: ProfileNameModalProps) {
  const [name, setName] = useState(profile?.name ?? "");
  const trimmed = name.trim();
  const disabled = pending || trimmed === "" || trimmed === profile?.name;

  return (
    <Modal open onOpenChange={(next) => !next && onClose()}>
      <ModalContent>
        <ModalTitleBlock
          title={profile ? "Rename profile" : "New routing profile"}
          description={
            profile
              ? "Give this routing profile a new name."
              : "Name a profile, then add per-domain rules to it."
          }
        />
        <ModalBody>
          <Field label="Name" htmlFor="routing-profile-name">
            <Input
              id="routing-profile-name"
              data-testid="routing-profile-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Streaming (UK)"
              autoFocus
            />
          </Field>
          {error != null && (
            <ApiErrorAlert error={error} fallback="Failed to save profile" />
          )}
        </ModalBody>
        <ModalFooter className="flex-col gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={onClose} disabled={pending}>
            Cancel
          </Button>
          <Button
            onClick={() => void onSubmit(trimmed)}
            disabled={disabled}
            data-testid="routing-profile-name-save"
          >
            {pending ? "Saving…" : profile ? "Save" : "Create"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
