import { apiErrorMessage, apiRequestId } from "@wardnet/wardnet-web";
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
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2.5 text-danger-soft-ink"
    >
      <CircleAlertIcon className="mt-0.5 size-4 shrink-0" />
      <div className="flex flex-col gap-0.5">
        <p className="text-sm">{message}</p>
        {requestId && <p className="font-mono text-[11px] text-ink-3">Request ID: {requestId}</p>}
      </div>
    </div>
  );
}
