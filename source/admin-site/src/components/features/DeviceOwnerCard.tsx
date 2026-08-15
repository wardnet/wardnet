import { Text } from "@wardnet/web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Field } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import type { MutationHandle } from "@/lib/mutationHandle";
import type { Device, User } from "@wardnet/js";

/** Sentinel for "nobody" — `Select` has no concept of a null option value. */
const UNASSIGNED = "__unassigned__";

interface DeviceOwnerCardProps {
  device: Device;
  users: User[];
  /** The page's hoisted owner-assignment mutation. */
  setOwner: MutationHandle<{ deviceId: string; ownerUserId: string | null }>;
}

/**
 * Whose device this is (ADR-0031 §4).
 *
 * **Attribution, never access.** Assigning an owner changes nothing about what
 * the device may do: device identity is derived from its source IP, so if
 * ownership granted an owner's privileges, admin access would collapse to IP
 * spoofing. The card says so in plain words, because "Owner: Ana (admin)" is
 * otherwise a very natural thing to misread as a permission.
 */
export function DeviceOwnerCard({
  device,
  users,
  setOwner,
}: DeviceOwnerCardProps) {
  const value = device.owner_user_id ?? UNASSIGNED;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Owner</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <Field
          label="Belongs to"
          help="Used for labelling only — who this device belongs to in lists and activity. It grants no access: the device can do exactly what its zone and rules allow, whoever owns it."
        >
          <Select
            value={value}
            disabled={setOwner.isPending}
            onValueChange={(next) => {
              void setOwner.mutateAsync({
                deviceId: device.id,
                ownerUserId: next === UNASSIGNED ? null : next,
              });
            }}
          >
            <SelectTrigger data-testid="device-owner">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={UNASSIGNED}>Nobody</SelectItem>
              {users.map((user) => (
                <SelectItem key={user.id} value={user.id}>
                  {user.display_name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        {users.length === 0 && (
          <Text size="xs" className="text-ink-3">
            No users yet. Add people under Household → Users to assign devices
            to them.
          </Text>
        )}
      </CardContent>
    </Card>
  );
}
