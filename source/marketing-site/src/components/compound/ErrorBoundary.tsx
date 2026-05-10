import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Root-level React error boundary. Catches exceptions thrown during render,
 * lifecycle, or event handlers of any descendant component and swaps the
 * subtree for a full-page Forge `.empty` block with a `.pill--down` danger
 * tag.
 *
 * Class component because React still only supports error boundaries as
 * classes (`getDerivedStateFromError` + `componentDidCatch`). No hook
 * equivalent is available as of React 19.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Log for dev console + any installed reporter. The UI only shows the
    // short message; the full stack + component trace stay in logs.
    // eslint-disable-next-line no-console
    console.error("ErrorBoundary caught:", error, info.componentStack);
  }

  handleRetry = () => {
    // Clear the error so the subtree re-renders. If clearing isn't enough
    // (e.g. the failure is in a module-level singleton), the user can fall
    // back to the browser refresh.
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      const message = this.state.error.message;
      return (
        <main className="col items-center justify-center min-h-screen p-6">
          <div className="empty col items-center gap-12 max-w-xl">
            <span className="pill pill--down">
              <AlertTriangle size={12} aria-hidden="true" />
              Unexpected error
            </span>
            <h1 className="h-title">Something broke on our end</h1>
            <p className="h-sub">
              The page hit an error while rendering. Try again — if it keeps happening, file an
              issue and include what you were doing.
            </p>
            {message ? <pre className="kbd">{message}</pre> : null}
            <div className="row gap-12 wrap">
              <button type="button" className="btn btn--primary" onClick={this.handleRetry}>
                <RefreshCw size={14} aria-hidden="true" />
                Try again
              </button>
              <a className="btn" href="https://github.com/wardnet/wardnet/issues/new">
                Report issue
              </a>
            </div>
          </div>
        </main>
      );
    }
    return this.props.children;
  }
}
