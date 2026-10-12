import * as React from "react";
import { ErrorState } from "./states";

/** One panel crashing must not take the workspace (or its siblings) down. */
export class PanelErrorBoundary extends React.Component<
  { name: string; children: React.ReactNode },
  { error: unknown }
> {
  override state: { error: unknown } = { error: null };

  static getDerivedStateFromError(error: unknown) {
    return { error };
  }

  override componentDidCatch(error: unknown, info: React.ErrorInfo) {
    console.error(`panel ${this.props.name} crashed`, error, info.componentStack);
  }

  override render() {
    if (this.state.error) {
      return (
        <ErrorState
          title={`${this.props.name} stopped working`}
          error={this.state.error}
          onRetry={() => this.setState({ error: null })}
        />
      );
    }
    return this.props.children;
  }
}
