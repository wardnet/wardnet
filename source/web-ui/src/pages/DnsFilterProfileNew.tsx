import { useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "@/components/core/ui/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { DetailPageHeader } from "@/components/compound/DetailPageHeader";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateDnsFilterProfile } from "@/hooks/useDnsFilter";

/** Routed create flow for a new DNS filter profile (admin only). */
export default function DnsFilterProfileNew() {
  const navigate = useNavigate();
  const create = useCreateDnsFilterProfile();
  const [name, setName] = useState("");

  async function handleSave() {
    const res = await create.mutateAsync({ name: name.trim() });
    void navigate(`/dns/filter/profiles/${res.profile.id}`);
  }

  function handleCancel() {
    void navigate("/dns/filter");
  }

  return (
    <div className="flex flex-col gap-6 p-4 md:p-6">
      <DetailPageHeader
        parentLabel="DNS Filtering"
        parentTo="/dns/filter"
        itemLabel="New profile"
      />

      <Card>
        <CardHeader>
          <CardTitle>New profile</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <Label htmlFor="profile-name">Name</Label>
            <Input
              id="profile-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Parental Controls"
              autoFocus
            />
            <p className="text-xs text-muted-foreground">
              You'll add blocklists, allowlist entries, and custom rules to this profile after it's
              created.
            </p>
          </div>

          {create.isError && (
            <ApiErrorAlert error={create.error} fallback="Failed to create profile" />
          )}
        </CardContent>
        <CardFooter className="justify-end gap-2">
          <Button variant="ghost" onClick={handleCancel} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={create.isPending || name.trim() === ""}>
            {create.isPending ? "Creating…" : "Create profile"}
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
