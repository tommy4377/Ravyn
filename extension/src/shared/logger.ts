const PREFIX = "[Ravyn]";

export const logger = {
  warn(message: string, details?: unknown): void {
    console.warn(PREFIX, message, details ?? "");
  },
  error(message: string, details?: unknown): void {
    console.error(PREFIX, message, details ?? "");
  },
};
