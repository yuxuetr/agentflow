import React from 'react';

// Q3.7.1: top-level React error boundary so an unhandled runtime
// error inside any page component falls back to a readable diagnostic
// panel instead of a white screen.
//
// Without this, *any* throw inside the render tree — a malformed SSE
// payload that slips past zod (Q3.7.2), a stale property access after a
// schema migration, an unexpected `null` from the API — wipes the
// entire `<App />` mount and leaves the operator staring at an empty
// page with nothing in the URL bar to indicate what happened.
//
// The fallback intentionally shows the error message + a short
// component-stack snippet inline (not just a generic "Something went
// wrong"). Operators run this UI to debug AgentFlow; hiding the error
// would defeat the point. The full stack stays in the browser console
// via `console.error` — same as the React default — so DevTools still
// surfaces every line.
//
// A "Reload" button gives the operator one-click recovery without
// having to find F5; a "Reset" button re-mounts the boundary's
// children without a full page reload (useful when the error fired
// during an effect and we just want React to retry render).

type ErrorBoundaryProps = {
  /** Children to render when no error is captured. */
  children: React.ReactNode;
  /**
   * Optional label embedded in the fallback heading so multiple
   * boundaries nested down the tree announce *which* tree blew up.
   * Defaults to "AgentFlow UI" for the top-level wrap.
   */
  label?: string;
};

type ErrorBoundaryState = {
  error: Error | null;
  componentStack: string | null;
};

export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    // React 16+ contract: return state patch synchronously so the next
    // render flips to the fallback UI in the same commit phase the
    // error was thrown in.
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    // Mirror React's default console behaviour so DevTools' error pane
    // still surfaces the full stack — operators run this UI for
    // *debugging*, hiding the stack would defeat the point.
    // eslint-disable-next-line no-console
    console.error('[ErrorBoundary] caught error:', error, info);
    this.setState({ componentStack: info.componentStack ?? null });
  }

  reset = (): void => {
    this.setState({ error: null, componentStack: null });
  };

  reload = (): void => {
    window.location.reload();
  };

  render(): React.ReactNode {
    if (this.state.error === null) {
      return this.props.children;
    }
    const label = this.props.label ?? 'AgentFlow UI';
    const error = this.state.error;
    const stack = this.state.componentStack ?? '(component stack not available)';
    return (
      <main className="error-boundary">
        <section className="error-boundary__panel" role="alert">
          <p className="error-boundary__kicker">AgentFlow</p>
          <h1>{label} ran into an unhandled error</h1>
          <p className="error-boundary__summary">{error.message || String(error)}</p>
          <details className="error-boundary__details" open>
            <summary>Component stack</summary>
            <pre>{stack.trim()}</pre>
          </details>
          <p className="error-boundary__hint">
            The full stack and React DevTools breadcrumb were written to the browser
            console. Use <strong>Reset</strong> to re-mount the tree without a full
            page reload, or <strong>Reload</strong> if state is unrecoverable.
          </p>
          <div className="error-boundary__actions">
            <button type="button" onClick={this.reset}>
              Reset
            </button>
            <button type="button" onClick={this.reload}>
              Reload page
            </button>
          </div>
        </section>
      </main>
    );
  }
}
