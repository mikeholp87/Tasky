import { Component, type ErrorInfo, type ReactNode } from "react";
import { outlineButton } from "@/components/ui/button";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** Contains a crash in one page so the shell (sidebar, top bar) keeps working. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Page crashed", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div role="alert" className="mx-auto max-w-lg py-16 text-center">
        <h1 className="text-lg font-semibold">This page stopped working</h1>
        <p className="mt-2 text-fg-muted">
          Something went wrong while showing it. Other pages still work.
        </p>
        <p className="selectable mt-3 break-words font-mono text-[0.8125rem] text-fg-subtle">
          {error.message}
        </p>
        <button
          type="button"
          className={`${outlineButton} mx-auto mt-5`}
          onClick={() => this.setState({ error: null })}
        >
          Reload page
        </button>
      </div>
    );
  }
}
