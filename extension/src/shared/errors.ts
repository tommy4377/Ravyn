import type { NativeError } from "./contracts";

export class RavynExtensionError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly retryable = false,
  ) {
    super(message);
    this.name = "RavynExtensionError";
  }

  toNativeError(): NativeError {
    return {
      code: this.code,
      message: this.message,
      retryable: this.retryable,
    };
  }
}

export function toExtensionError(
  error: unknown,
  fallbackCode = "UNEXPECTED_ERROR",
): RavynExtensionError {
  if (error instanceof RavynExtensionError) return error;
  if (error instanceof Error)
    return new RavynExtensionError(fallbackCode, error.message, true);
  return new RavynExtensionError(fallbackCode, String(error), true);
}
