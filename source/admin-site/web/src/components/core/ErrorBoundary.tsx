import { Component, type ErrorInfo, type ReactNode } from "react";
import { logger } from "@wardnet/wardnet-web";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Button } from "@wardnet/forge-web/button";

interface Props {
  children: ReactNode;
  /** Custom fallback. Defaults to the built-in error card. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Top-level error boundary. Catches render-phase exceptions and shows a
 * recoverable error card instead of a blank screen.
 *
 * Mount once near the root (e.g. wrapping <AppLayout>). For section-level
 * isolation, use the `fallback` prop to provide a smaller inline message.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    logger.error("[ErrorBoundary]", error, info.componentStack);
  }

  reset = () => {
    this.setState({ error: null });
  };

  render() {
    const { error } = this.state;
    const { children, fallback } = this.props;

    if (error) {
      if (fallback) return fallback(error, this.reset);
      return <DefaultErrorCard error={error} onReset={this.reset} />;
    }

    return children;
  }
}

function DefaultErrorCard({ error, onReset }: { error: Error; onReset: () => void }) {
  return (
    <div className="flex min-h-[40vh] items-center justify-center p-8">
      <Card className="w-full max-w-md border-destructive/40">
        <CardHeader>
          <CardTitle className="text-destructive">Something went wrong</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <p className="text-sm text-ink-3">
            An unexpected error occurred. You can try recovering, or reload the page.
          </p>
          <p className="rounded bg-muted px-3 py-2 font-mono text-xs text-ink-2 break-all">
            {error.message}
          </p>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={onReset}>
              Try again
            </Button>
            <Button variant="ghost" size="sm" onClick={() => window.location.reload()}>
              Reload page
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
