import { useState } from "react";
import { Card, CardContent } from "@/components/core/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { PageHeader } from "@/components/compound/PageHeader";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import { BlocklistTable } from "@/components/compound/BlocklistTable";
import { AllowlistTable } from "@/components/compound/AllowlistTable";
import { FilterRuleTable } from "@/components/compound/FilterRuleTable";
import { BlocklistSheet } from "@/components/features/BlocklistSheet";
import { CreateAllowlistSheet } from "@/components/features/CreateAllowlistSheet";
import { CreateFilterRuleSheet } from "@/components/features/CreateFilterRuleSheet";
import {
  useBlocklists,
  useDeleteBlocklist,
  useUpdateBlocklist,
  useUpdateBlocklistNow,
  useAllowlist,
  useDeleteAllowlistEntry,
  useFilterRules,
  useUpdateFilterRule,
  useDeleteFilterRule,
} from "@/hooks/useDns";
import type { Blocklist } from "@wardnet/js";

/** Ad blocking management page (admin only). */
export default function AdBlocking() {
  const { data: blocklistData, isLoading: blocklistsLoading } = useBlocklists();
  const { data: allowlistData } = useAllowlist();
  const { data: filterRulesData } = useFilterRules();

  const deleteBlocklist = useDeleteBlocklist();
  const updateBlocklist = useUpdateBlocklist();
  const updateBlocklistNow = useUpdateBlocklistNow();
  const deleteAllowlistEntry = useDeleteAllowlistEntry();
  const updateFilterRule = useUpdateFilterRule();
  const deleteFilterRule = useDeleteFilterRule();

  const [blocklistSheetOpen, setBlocklistSheetOpen] = useState(false);
  const [editingBlocklist, setEditingBlocklist] = useState<Blocklist | null>(null);
  const [addAllowlistOpen, setAddAllowlistOpen] = useState(false);
  const [addFilterRuleOpen, setAddFilterRuleOpen] = useState(false);

  const [deleteBlocklistId, setDeleteBlocklistId] = useState<string | null>(null);
  const [deleteAllowlistId, setDeleteAllowlistId] = useState<string | null>(null);
  const [deleteFilterRuleId, setDeleteFilterRuleId] = useState<string | null>(null);

  const blocklists = blocklistData?.blocklists ?? [];
  const allowlist = allowlistData?.entries ?? [];
  const filterRules = filterRulesData?.rules ?? [];

  const blocklistToDelete = blocklists.find((b) => b.id === deleteBlocklistId);
  const allowlistEntryToDelete = allowlist.find((e) => e.id === deleteAllowlistId);
  const filterRuleToDelete = filterRules.find((r) => r.id === deleteFilterRuleId);

  return (
    <>
      <PageHeader title="Ad Blocking" />

      {blocklistsLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading ad blocking configuration...
          </CardContent>
        </Card>
      )}

      {!blocklistsLoading && (
        <Tabs defaultValue="blocklists" className="flex min-h-0 flex-1 flex-col">
          <TabsList>
            <TabsTrigger value="blocklists">
              Blocklists
              {blocklists.length > 0 && (
                <span className="ml-1.5 rounded-full bg-muted px-1.5 py-0.5 text-xs tabular-nums">
                  {blocklists.length}
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="allowlist">
              Allowlist
              {allowlist.length > 0 && (
                <span className="ml-1.5 rounded-full bg-muted px-1.5 py-0.5 text-xs tabular-nums">
                  {allowlist.length}
                </span>
              )}
            </TabsTrigger>
            <TabsTrigger value="rules">
              Custom Rules
              {filterRules.length > 0 && (
                <span className="ml-1.5 rounded-full bg-muted px-1.5 py-0.5 text-xs tabular-nums">
                  {filterRules.length}
                </span>
              )}
            </TabsTrigger>
          </TabsList>

          <TabsContent value="blocklists" className="mt-4 flex min-h-0 flex-1 flex-col">
            <BlocklistTable
              blocklists={blocklists}
              refreshingId={
                updateBlocklistNow.isPending ? (updateBlocklistNow.variables ?? null) : null
              }
              onRefresh={(id) => updateBlocklistNow.mutate(id)}
              onToggle={(b) => updateBlocklist.mutate({ id: b.id, body: { enabled: !b.enabled } })}
              onEdit={(b) => {
                setEditingBlocklist(b);
                setBlocklistSheetOpen(true);
              }}
              onDelete={setDeleteBlocklistId}
              onAdd={() => {
                setEditingBlocklist(null);
                setBlocklistSheetOpen(true);
              }}
            />
          </TabsContent>

          <TabsContent value="allowlist" className="mt-4 flex min-h-0 flex-1 flex-col">
            <AllowlistTable
              entries={allowlist}
              onDelete={setDeleteAllowlistId}
              onAdd={() => setAddAllowlistOpen(true)}
            />
          </TabsContent>

          <TabsContent value="rules" className="mt-4 flex min-h-0 flex-1 flex-col">
            <FilterRuleTable
              rules={filterRules}
              onToggle={(id, enabled) => updateFilterRule.mutate({ id, body: { enabled } })}
              onDelete={setDeleteFilterRuleId}
              onAdd={() => setAddFilterRuleOpen(true)}
            />
          </TabsContent>
        </Tabs>
      )}

      {/* Create/edit sheets */}
      <BlocklistSheet
        key={editingBlocklist?.id ?? "new"}
        open={blocklistSheetOpen}
        onOpenChange={(open) => {
          setBlocklistSheetOpen(open);
          if (!open) setEditingBlocklist(null);
        }}
        blocklist={editingBlocklist}
      />
      <CreateAllowlistSheet open={addAllowlistOpen} onOpenChange={setAddAllowlistOpen} />
      <CreateFilterRuleSheet open={addFilterRuleOpen} onOpenChange={setAddFilterRuleOpen} />

      {/* Confirm: delete blocklist */}
      <ConfirmDialog
        open={!!deleteBlocklistId}
        onOpenChange={(open) => {
          if (!open) setDeleteBlocklistId(null);
        }}
        title="Delete blocklist"
        description={`Delete "${blocklistToDelete?.name ?? "this blocklist"}"? It will no longer be downloaded or used for filtering.`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteBlocklistId) deleteBlocklist.mutate(deleteBlocklistId);
          setDeleteBlocklistId(null);
        }}
      />

      {/* Confirm: remove allowlist entry */}
      <ConfirmDialog
        open={!!deleteAllowlistId}
        onOpenChange={(open) => {
          if (!open) setDeleteAllowlistId(null);
        }}
        title="Remove allowlist entry"
        description={`Remove "${allowlistEntryToDelete?.domain ?? "this domain"}" from the allowlist? It may be blocked again if it appears in a blocklist.`}
        confirmLabel="Remove"
        onConfirm={() => {
          if (deleteAllowlistId) deleteAllowlistEntry.mutate(deleteAllowlistId);
          setDeleteAllowlistId(null);
        }}
      />

      {/* Confirm: delete filter rule */}
      <ConfirmDialog
        open={!!deleteFilterRuleId}
        onOpenChange={(open) => {
          if (!open) setDeleteFilterRuleId(null);
        }}
        title="Delete filter rule"
        description={`Delete rule "${filterRuleToDelete?.rule_text ?? ""}"?`}
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleteFilterRuleId) deleteFilterRule.mutate(deleteFilterRuleId);
          setDeleteFilterRuleId(null);
        }}
      />
    </>
  );
}
