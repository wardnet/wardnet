import { Button } from "@wardnet/forge-web/button";
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@wardnet/forge-web/dropdown-menu";
import { Pill } from "@wardnet/forge-web/pill";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/forge-web/select";
import { DownloadIcon, MoreHorizontal, PauseIcon, PlayIcon, Trash2 } from "lucide-react";
import { LogViewer } from "@/components/compound/LogViewer";
import { useLogStore } from "@/stores/logStore";

const LEVELS = ["trace", "debug", "info", "warn", "error"] as const;

/** Dashboard widget showing a live-streaming, filterable log tail.
 *
 *  Responsive header:
 *  - Desktop (`md+`): Level select + Pause/Resume + Clear + Download
 *    all inline next to the streaming pill.
 *  - Mobile: Level filter stays inline (it's the primary control);
 *    Pause/Clear/Download collapse into a `…` overflow menu so the
 *    row doesn't blow past the card edge. */
export function DashboardLogWidget() {
  const { entries, connected, paused, skipped, filter, setFilter, clear, setPaused } =
    useLogStore();

  function handleLevelChange(newLevel: string) {
    setFilter({ ...filter, level: newLevel });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Live logs</CardTitle>
        <Pill variant={connected ? "ok" : "down"}>
          <span className="dot" aria-hidden="true" />
          {connected ? "Streaming" : "Disconnected"}
        </Pill>
        <CardAction className="flex flex-wrap items-center gap-2">
          <Select value={filter.level ?? "info"} onValueChange={handleLevelChange}>
            <SelectTrigger className="select-trigger--sm w-24">
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

          {/* Desktop: full button row visible. Wrapped in a div so
              Tailwind's responsive display wins against Forge's
              unlayered `.btn { display: inline-flex }`. */}
          <div className="hidden md:contents">
            <Button variant="ghost" size="sm" onClick={() => setPaused(!paused)}>
              {paused ? <PlayIcon /> : <PauseIcon />}
              {paused ? "Resume" : "Pause"}
            </Button>
            <Button variant="ghost" size="sm" onClick={clear}>
              <Trash2 />
              Clear
            </Button>
            <Button variant="ghost" size="sm" asChild>
              <a href="/api/system/logs/download" download>
                <DownloadIcon />
                Download
              </a>
            </Button>
          </div>

          {/* Mobile: same actions collapsed into an overflow menu so
              the toolbar stays a single 28px line at narrow widths. */}
          <div className="md:hidden">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label="Log actions">
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem onSelect={() => setPaused(!paused)}>
                  {paused ? <PlayIcon aria-hidden /> : <PauseIcon aria-hidden />}
                  <span>{paused ? "Resume" : "Pause"}</span>
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={clear}>
                  <Trash2 aria-hidden />
                  <span>Clear</span>
                </DropdownMenuItem>
                <DropdownMenuItem asChild>
                  <a href="/api/system/logs/download" download>
                    <DownloadIcon aria-hidden />
                    <span>Download</span>
                  </a>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </CardAction>
      </CardHeader>
      <CardContent>
        <LogViewer entries={entries} connected={connected} skipped={skipped} maxHeight="20rem" />
      </CardContent>
    </Card>
  );
}
