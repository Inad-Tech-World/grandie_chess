import { notifications } from "@mantine/notifications";
import { commands } from "../bindings";

export { commands };
export * from "../bindings";

type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: string };

/** Unwraps a command result, throwing the Rust error message on failure. */
export async function unwrap<T>(call: Promise<CommandResult<T>>): Promise<T> {
  const result = await call;
  if (result.status === "error") {
    throw new Error(result.error);
  }
  return result.data;
}

/** Shows an error notification and returns undefined instead of throwing. */
export async function tryCommand<T>(call: Promise<CommandResult<T>>, title = "Something went wrong"): Promise<T | undefined> {
  try {
    return await unwrap(call);
  } catch (e) {
    notifyError(title, e);
    return undefined;
  }
}

export function notifyError(title: string, e: unknown) {
  const message = e instanceof Error ? e.message : String(e);
  notifications.show({ color: "red", title, message, autoClose: 8000 });
}
