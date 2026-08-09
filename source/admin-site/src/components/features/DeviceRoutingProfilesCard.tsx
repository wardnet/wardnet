import { useState } from "react";
import { ArrowDown, ArrowUp, X } from "lucide-react";
import { Button } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { Text } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import type { MutationHandle } from "@/lib/mutationHandle";
import type { Device, RoutingProfile } from "@wardnet/js";

interface DeviceRoutingProfilesCardProps {
  device: Device;
  /** All routing profiles. */
  allProfiles: RoutingProfile[];
  /** This device's assigned profile ids, in priority order. */
  assignedIds: string[];
  /** The page's hoisted assignment-save mutation. */
  save: MutationHandle<{ deviceId: string; profileIds: string[] }>;
}

/**
 * Per-device routing-profile assignment (issue #241). A device's assigned
 * profiles are consulted in priority order — the first profile whose rule
 * matches a resolved domain wins — so the list order matters. Editing mirrors
 * the upstream-DNS reorder idiom: move up/down + remove, add from the
 * unassigned set, then save the whole ordered list. Pure presentation — the
 * owning page wires the query/mutation hooks and passes data + callbacks in.
 */
export function DeviceRoutingProfilesCard({
  device,
  allProfiles,
  assignedIds,
  save,
}: DeviceRoutingProfilesCardProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<string[]>([]);

  const byId = (id: string): RoutingProfile | undefined =>
    allProfiles.find((p) => p.id === id);
  const nameOf = (id: string): string => byId(id)?.name ?? id;

  function startEdit() {
    setDraft(assignedIds);
    save.reset();
    setEditing(true);
  }

  function move(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= draft.length) return;
    const next = [...draft];
    // eslint-disable-next-line security/detect-object-injection -- reorder swap on a local array copy; both indices are row positions and `target` is bounds-checked above
    [next[index], next[target]] = [next[target], next[index]];
    setDraft(next);
  }

  function removeAt(index: number) {
    setDraft(draft.filter((_, i) => i !== index));
  }

  function add(profileId: string) {
    if (!draft.includes(profileId)) setDraft([...draft, profileId]);
  }

  async function handleSave() {
    await save.mutateAsync({ deviceId: device.id, profileIds: draft });
    setEditing(false);
  }

  const unassigned = allProfiles.filter((p) => !draft.includes(p.id));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Routing profiles</CardTitle>
        {!editing && (
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={startEdit}
              disabled={allProfiles.length === 0}
              data-testid="device-routing-profiles-edit"
            >
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-4">
            {draft.length === 0 ? (
              <Text size="sm" className="text-ink-3">
                No profiles assigned. Add one below.
              </Text>
            ) : (
              <div className="flex flex-col divide-y divide-line">
                {draft.map((profileId, index) => (
                  <div
                    key={profileId}
                    data-testid="device-routing-profile-row"
                    className="flex items-center gap-3 py-2"
                  >
                    <Text size="xs" className="w-5 text-ink-3">
                      {index + 1}.
                    </Text>
                    <Text className="min-w-0 flex-1 truncate">
                      {nameOf(profileId)}
                    </Text>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label="Move up"
                      disabled={index === 0}
                      onClick={() => move(index, -1)}
                    >
                      <ArrowUp className="size-4" aria-hidden />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label="Move down"
                      disabled={index === draft.length - 1}
                      onClick={() => move(index, 1)}
                    >
                      <ArrowDown className="size-4" aria-hidden />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      aria-label="Remove"
                      onClick={() => removeAt(index)}
                    >
                      <X className="size-4" aria-hidden />
                    </Button>
                  </div>
                ))}
              </div>
            )}

            {unassigned.length > 0 && (
              <Field label="Add profile">
                <Select value="" onValueChange={add}>
                  <SelectTrigger data-testid="device-routing-profile-add">
                    <SelectValue placeholder="Choose a profile…" />
                  </SelectTrigger>
                  <SelectContent>
                    {unassigned.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            )}

            <Text size="xs" className="text-ink-3">
              Profiles are consulted top-to-bottom — the first matching rule
              wins.
            </Text>

            {save.isError && (
              <ApiErrorAlert
                error={save.error}
                fallback="Failed to update routing profiles"
              />
            )}
          </CardContent>
          <FormActions
            secondaryLabel="Cancel"
            secondaryProps={{
              onClick: () => setEditing(false),
              disabled: save.isPending,
            }}
            primaryLabel={save.isPending ? "Saving…" : "Save"}
            primaryProps={{
              onClick: handleSave,
              disabled: save.isPending,
              "data-testid": "device-routing-profiles-save",
            }}
          />
        </>
      ) : (
        <CardContent>
          {assignedIds.length === 0 ? (
            <Text size="sm" className="text-ink-3">
              {allProfiles.length === 0
                ? "No routing profiles exist yet."
                : "No routing profiles assigned."}
            </Text>
          ) : (
            <ol className="flex flex-col gap-1">
              {assignedIds.map((profileId, index) => (
                <li key={profileId} className="flex items-center gap-2">
                  <Text size="xs" className="text-ink-3">
                    {index + 1}.
                  </Text>
                  <Text size="sm">{nameOf(profileId)}</Text>
                </li>
              ))}
            </ol>
          )}
        </CardContent>
      )}
    </Card>
  );
}
