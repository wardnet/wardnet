import { Button } from "@wardnet/forge-web/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/forge-web/select";
import { DownloadIcon, PauseIcon, PlayIcon } from "lucide-react";
import { LogViewer } from "@/components/compound/LogViewer";
import { useLogStore } from "@/stores/logStore";

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
      <CardHeader>
        <CardTitle>Logs</CardTitle>
        <CardAction className="flex flex-wrap items-center gap-2">
          <Select value={filter.level ?? "info"} onValueChange={handleLevelChange}>
            <SelectTrigger className="w-24">
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
          <Button variant="ghost" size="sm" onClick={() => setPaused(!paused)}>
            {paused ? <PlayIcon /> : <PauseIcon />}
            {paused ? "Resume" : "Pause"}
          </Button>
          <Button variant="ghost" size="sm" onClick={clear}>
            Clear
          </Button>
          <Button variant="ghost" size="sm" asChild>
            <a href="/api/system/logs/download" download>
              <DownloadIcon />
              Download
            </a>
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <LogViewer entries={entries} connected={connected} skipped={skipped} maxHeight="20rem" />
      </CardContent>
    </Card>
  );
}
