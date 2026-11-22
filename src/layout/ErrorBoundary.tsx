import { Alert, Button, Code, Group, Stack } from "@mantine/core";
import { IconAlertTriangle, IconRefresh } from "@tabler/icons-react";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { commands } from "../bindings";

/** Write a UI error to the app's error log (best effort). */
export function logUiError(context: string, error: unknown, extra = ""): void {
  const message = error instanceof Error ? `${error.message}\n${error.stack ?? ""}` : String(error);
  void Promise.resolve()
    .then(() => commands.appLogError(context, extra ? `${message}\n${extra}` : message))
    .catch(() => {});
}

interface Props {
  /** Which screen this guards, for the log. */
  name: string;
  /** Outside the Mantine provider (the app's root): a plain fallback. */
  plain?: boolean;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/** Keeps one screen's error from blanking the whole window: the screen is
 *  replaced by a message, the rest of the app keeps working, and the error
 *  goes to the log. Give it a `key` per screen so switching away resets it. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    logUiError(`screen ${this.props.name}`, error, info.componentStack ?? "");
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    if (this.props.plain) {
      return (
        <div role="alert" style={{ padding: 32, fontFamily: "system-ui, sans-serif" }}>
          <h2>Grandie Chess ran into a problem</h2>
          <p>The details were saved to the error log.</p>
          <pre style={{ whiteSpace: "pre-wrap" }}>{error.message}</pre>
          <button onClick={() => window.location.reload()}>Reload</button>
        </div>
      );
    }
    return (
      <Stack p="xl" maw={640}>
        <Alert color="red" variant="light" icon={<IconAlertTriangle size={18} />} title="This screen ran into a problem" role="alert">
          <Stack gap="sm">
            <span>The rest of the app still works. The details were saved to the error log.</span>
            <Code block>{error.message}</Code>
            <Group>
              <Button size="xs" variant="light" leftSection={<IconRefresh size={14} />} onClick={() => this.setState({ error: null })}>
                Try again
              </Button>
            </Group>
          </Stack>
        </Alert>
      </Stack>
    );
  }
}
