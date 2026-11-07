import { Component, ErrorInfo, ReactNode } from "react";
import { AlertTriangle, RefreshCw, Copy, Check } from "lucide-react";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
  copied: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
    errorInfo: null,
    copied: false,
  };

  public static getDerivedStateFromError(error: Error): State {
    return {
      hasError: true,
      error,
      errorInfo: null,
      copied: false,
    };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Uncaught runtime error in React UI:", error, errorInfo);
    this.setState({ errorInfo });
  }

  private handleCopy = async () => {
    const { error, errorInfo } = this.state;
    const text = `[NovaProxy Error Report]\nError: ${error?.message}\nStack: ${error?.stack}\nComponent Stack: ${errorInfo?.componentStack}`;
    try {
      await navigator.clipboard.writeText(text);
      this.setState({ copied: true });
      setTimeout(() => this.setState({ copied: false }), 2000);
    } catch {
      // Fallback
    }
  };

  private handleReload = () => {
    window.location.reload();
  };

  private handleReset = () => {
    this.setState({
      hasError: false,
      error: null,
      errorInfo: null,
      copied: false,
    });
  };

  public render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="flex h-screen w-screen flex-col items-center justify-center bg-background p-6 text-foreground select-none">
          <div className="w-full max-w-xl rounded-xl border border-red-500/30 bg-card p-6 shadow-2xl backdrop-blur-md">
            <div className="flex items-center gap-3 text-red-500 mb-4">
              <div className="p-2.5 rounded-lg bg-red-500/10 border border-red-500/20">
                <AlertTriangle className="h-6 w-6" />
              </div>
              <div>
                <h2 className="text-lg font-bold text-foreground">
                  UI Rendering Error Occurred
                </h2>
                <p className="text-xs text-muted-foreground">
                  NovaProxy caught an unhandled client exception
                </p>
              </div>
            </div>

            <div className="mb-4 rounded-lg bg-muted/60 p-3 font-mono text-xs text-red-400 overflow-x-auto max-h-48 border border-border">
              <p className="font-bold">{this.state.error?.toString()}</p>
              {this.state.errorInfo?.componentStack && (
                <pre className="mt-2 text-[11px] text-muted-foreground whitespace-pre-wrap">
                  {this.state.errorInfo.componentStack}
                </pre>
              )}
            </div>

            <div className="flex items-center justify-between pt-2">
              <button
                onClick={this.handleCopy}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border bg-secondary hover:bg-secondary/80 text-xs font-medium transition-colors"
              >
                {this.state.copied ? (
                  <>
                    <Check className="h-3.5 w-3.5 text-emerald-500" />
                    <span>Copied Details</span>
                  </>
                ) : (
                  <>
                    <Copy className="h-3.5 w-3.5" />
                    <span>Copy Error Report</span>
                  </>
                )}
              </button>

              <div className="flex items-center gap-2">
                <button
                  onClick={this.handleReset}
                  className="px-3 py-1.5 rounded-lg border border-border hover:bg-accent text-xs font-medium transition-colors"
                >
                  Try Again
                </button>
                <button
                  onClick={this.handleReload}
                  className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground text-xs font-semibold shadow transition-all"
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                  <span>Reload App</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
