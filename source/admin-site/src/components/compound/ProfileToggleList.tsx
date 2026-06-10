import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import type { DnsFilterProfile } from "@wardnet/js";

interface ProfileToggleListProps {
  /** Catalog of profiles to render rows for. */
  profiles: DnsFilterProfile[];
  /** Currently-selected profile ids. Ids that aren't in `profiles` are
   *  preserved through `onChange` (i.e. orphans aren't dropped on toggle)
   *  but render no row — the caller usually wants to await their resolution. */
  selectedIds: string[];
  /** Called with the new full set when a row toggles. */
  onChange: (ids: string[]) => void;
  /** Disables every row's switch (e.g. when an upstream kill switch is off). */
  disabled?: boolean;
  /** Replaces the empty-state copy when `profiles` is empty. */
  emptyMessage?: string;
  /** Id of the external `Field` label that names this group, for screen-reader
   *  group association. Pairs with `role="group"` on the container. */
  ariaLabelledBy?: string;
}

/** Toggle list of DNS filter profiles. Each row is a labeled `Toggle`; the
 *  callback is fired with the resulting full id list (add or remove). The
 *  caller owns the surrounding label and any explanatory hint. */
export function ProfileToggleList({
  profiles,
  selectedIds,
  onChange,
  disabled = false,
  emptyMessage = "No profiles defined.",
  ariaLabelledBy,
}: ProfileToggleListProps) {
  function toggle(profileId: string, checked: boolean) {
    onChange(
      checked
        ? Array.from(new Set([...selectedIds, profileId]))
        : selectedIds.filter((p) => p !== profileId),
    );
  }

  if (profiles.length === 0) {
    return <p className="text-sm text-ink-3">{emptyMessage}</p>;
  }

  return (
    <div
      role="group"
      aria-labelledby={ariaLabelledBy}
      className="flex flex-col gap-2"
    >
      {profiles.map((profile) => (
        <ProfileToggleRow
          key={profile.id}
          profile={profile}
          checked={selectedIds.includes(profile.id)}
          disabled={disabled}
          onChange={(c) => toggle(profile.id, c)}
        />
      ))}
    </div>
  );
}

interface ProfileToggleRowProps {
  profile: DnsFilterProfile;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}

function ProfileToggleRow({
  profile,
  checked,
  disabled,
  onChange,
}: ProfileToggleRowProps) {
  return (
    <div className="flex items-center justify-between rounded-md border border-line px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="text-sm">{profile.name}</span>
        {profile.builtin && (
          <Pill variant="ghost" className="text-xs">
            Builtin
          </Pill>
        )}
      </div>
      <Toggle
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled}
        aria-label={profile.name}
      />
    </div>
  );
}
