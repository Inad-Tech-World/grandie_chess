import { notifications } from "@mantine/notifications";
import { commands, type ErrorKind, type ErrorPayload } from "../bindings";

export { commands };
export * from "../bindings";

type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: ErrorPayload };

/** An error from a Rust command: the message plus a kind the UI can act on. */
export class AppError extends Error {
  readonly kind: ErrorKind;
  constructor(payload: ErrorPayload) {
    super(payload.message);
    this.name = "AppError";
    this.kind = payload.kind;
  }
}

function isPayload(e: unknown): e is ErrorPayload {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

/** The readable message of anything thrown or returned as an error. */
export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (isPayload(e)) return e.message;
  return String(e);
}

/** The kind of a command error, if it is one. */
export function errorKind(e: unknown): ErrorKind | null {
  if (e instanceof AppError) return e.kind;
  if (isPayload(e)) return e.kind;
  return null;
}

/** Unwraps a command result, throwing an [`AppError`] on failure. */
export async function unwrap<T>(call: Promise<CommandResult<T>>): Promise<T> {
  const result = await call;
  if (result.status === "error") {
    throw new AppError(result.error);
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
  if (errorKind(e) === "cancelled") return;
  notifications.show({ color: "red", title, message: errorMessage(e), autoClose: 8000 });
}
