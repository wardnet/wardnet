import { apiErrorMessage, apiRequestId } from "@/lib/utils";
import { CircleAlertIcon } from "lucide-react";

interface ApiErrorAlertProps {
  error: unknown;
  fallback?: string;
}

/** Displays an API error with the request ID for log correlation. */
export function ApiErrorAlert({ error, fallback }: ApiErrorAlertProps) {
  const message = apiErrorMessage(error, fallback);
  const requestId = apiRequestId(error);

  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2.5">
      <CircleAlertIcon className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="flex flex-col gap-0.5">
        <p className="text-sm text-destructive">{message}</p>
        {requestId && (
          <p className="font-mono text-[11px] text-muted-foreground">Request ID: {requestId}</p>
        )}
      </div>
    </div>
  );
}
