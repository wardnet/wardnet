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
    <div className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 px-3 py-2.5">
      <CircleAlertIcon className="mt-0.5 size-4 shrink-0 text-danger" />
      <div className="flex flex-col gap-0.5">
        <p className="text-sm text-danger">{message}</p>
        {requestId && <p className="font-mono text-[11px] text-ink-3">Request ID: {requestId}</p>}
      </div>
    </div>
  );
}
