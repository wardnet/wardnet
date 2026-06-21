import { apiErrorMessage, apiRequestId } from "../lib/utils";
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
    <div role="alert" className="api-error-alert">
      <CircleAlertIcon className="api-error-alert__icon" />
      <div className="api-error-alert__body">
        <p className="api-error-alert__message">{message}</p>
        {requestId && (
          <p className="api-error-alert__request-id">Request ID: {requestId}</p>
        )}
      </div>
    </div>
  );
}
