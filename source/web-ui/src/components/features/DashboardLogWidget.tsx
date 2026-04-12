import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@/components/core/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { LogViewer } from "@/components/compound/LogViewer";
import { useLogStore } from "@/stores/logStore";
import { Download, Pause, Play } from "lucide-react";

const LEVELS = ["trace", "debug", "info", "warn", "error"] as const;

/** Dashboard widget showing a live-streaming, filterable log tail. */
export function DashboardLogWidget() {
  const { entries, connected, paused, skipped, filter, setFilter, clear, setPaused } =
    useLogStore();

  function handleLevelChange(newLevel: string) {
    setFilter({ ...filter, level: newLevel });
  }

  return (
    <Card>
      <CardHeader className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <CardTitle className="text-sm font-semibold">Logs</CardTitle>
        <div className="flex flex-wrap items-center gap-2">
          <Select value={filter.level ?? "info"} onValueChange={handleLevelChange}>
            <SelectTrigger className="h-8 w-24 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LEVELS.map((l) => (
                <SelectItem key={l} value={l}>
                  {l.toUpperCase()}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setPaused(!paused)}
            className="h-8 text-xs"
          >
            {paused ? <Play className="mr-1 size-3" /> : <Pause className="mr-1 size-3" />}
            {paused ? "Resume" : "Pause"}
          </Button>
          <Button variant="ghost" size="sm" onClick={clear} className="h-8 text-xs">
            Clear
          </Button>
          <Button variant="ghost" size="sm" className="h-8 text-xs" asChild>
            <a href="/api/system/logs/download" download>
              <Download className="mr-1 size-3 sm:hidden" />
              <span className="hidden sm:inline">Download</span>
              <span className="sm:hidden">Logs</span>
            </a>
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <LogViewer entries={entries} connected={connected} skipped={skipped} maxHeight="20rem" />
      </CardContent>
    </Card>
  );
}
