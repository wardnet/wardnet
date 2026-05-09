import { Component, type ErrorInfo, type ReactNode } from "react";
import { ErrorView } from "@/pages/ErrorView";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Root-level React error boundary. Catches exceptions thrown during render,
 * lifecycle, or event handlers of any descendant component and swaps the
 * subtree for a styled [`ErrorView`].
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
    // Clear the error so the subtree re-renders. Callers that want a full
    // reload can pass their own handler via the <ErrorView> they render.
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      return <ErrorView message={this.state.error.message} onRetry={this.handleRetry} />;
    }
    return this.props.children;
  }
}
