import { Component, type ReactNode } from "react";

/** Shows a message instead of a blank page when a view throws; reset by changing `key`. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  override state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  override render() {
    if (this.state.error) {
      return (
        <div className="enc-page-error" role="alert">
          <h1>Something went wrong</h1>
          <p>This page could not be shown ({this.state.error.message}).</p>
        </div>
      );
    }
    return this.props.children;
  }
}
