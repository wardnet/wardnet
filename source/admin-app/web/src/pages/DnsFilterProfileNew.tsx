import { useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
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
    <div className="col gap-20">
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
          <Field
            label="Name"
            htmlFor="profile-name"
            help="You'll add blocklists, allowlist entries, and custom rules to this profile after it's created."
          >
            <Input
              id="profile-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Parental Controls"
              autoFocus
            />
          </Field>

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
