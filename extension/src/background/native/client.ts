import {
  NATIVE_HOST_NAME,
  NATIVE_PROTOCOL_MIN,
  NATIVE_PROTOCOL_VERSION,
  protocolCompatible,
  type ConnectionStatus,
  type NativeCapabilities,
  type NativeCommand,
  type NativeEvent,
  type NativeRequest,
  type NativeResponse,
} from "../../shared/contracts";
import { RavynExtensionError, toExtensionError } from "../../shared/errors";
import { logger } from "../../shared/logger";

const REQUEST_TIMEOUT_MS = 30_000;
const RECONNECT_DELAYS_MS = [250, 1_000, 3_000, 10_000, 30_000];
const HEARTBEAT_INTERVAL_MS = 15_000;

interface PendingRequest {
  resolve(value: unknown): void;
  reject(error: unknown): void;
  timer: number;
}

export class NativeClient {
  private port: browser.runtime.Port | null = null;
  private pending = new Map<string, PendingRequest>();
  private connecting: Promise<void> | null = null;
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;
  private heartbeatTimer: number | null = null;
  private statusValue: ConnectionStatus = {
    hostAvailable: false,
    backendConnected: false,
  };
  private statusListeners = new Set<(status: ConnectionStatus) => void>();
  private eventListeners = new Set<(event: NativeEvent) => void>();

  get status(): ConnectionStatus {
    return this.statusValue;
  }

  subscribeStatus(listener: (status: ConnectionStatus) => void): () => void {
    this.statusListeners.add(listener);
    listener(this.statusValue);
    return () => this.statusListeners.delete(listener);
  }

  subscribeEvents(listener: (event: NativeEvent) => void): () => void {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  async connect(): Promise<void> {
    if (this.port) return;
    if (this.connecting) return this.connecting;
    // A retry is already scheduled after a prior failure — let the backoff
    // run its course instead of hammering the host every time something
    // (popup polling, the heartbeat) happens to call connect()/request() in
    // between. request() surfaces NATIVE_HOST_UNAVAILABLE while this holds.
    if (this.reconnectTimer !== null) return;
    this.connecting = this.openPort().finally(() => {
      this.connecting = null;
    });
    return this.connecting;
  }

  disconnect(): void {
    if (this.reconnectTimer !== null) window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    this.stopHeartbeat();
    this.port?.disconnect();
    this.port = null;
    this.rejectPending(
      new RavynExtensionError(
        "NATIVE_DISCONNECTED",
        "The Ravyn native connection closed.",
        true,
      ),
    );
  }

  async request<T>(command: NativeCommand, payload: unknown = {}): Promise<T> {
    await this.connect();
    const port = this.port;
    if (!port)
      throw new RavynExtensionError(
        "NATIVE_HOST_UNAVAILABLE",
        "Ravyn browser integration is unavailable.",
        true,
      );
    const id = crypto.randomUUID();
    const request: NativeRequest = {
      id,
      protocolVersion: NATIVE_PROTOCOL_VERSION,
      command,
      payload,
    };
    return new Promise<T>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.pending.delete(id);
        reject(
          new RavynExtensionError(
            "NATIVE_TIMEOUT",
            `Ravyn did not answer ${command} in time.`,
            true,
          ),
        );
      }, REQUEST_TIMEOUT_MS);
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        timer,
      });
      try {
        port.postMessage(request);
      } catch (error) {
        window.clearTimeout(timer);
        this.pending.delete(id);
        reject(toExtensionError(error, "NATIVE_SEND_FAILED"));
      }
    });
  }

  async refreshStatus(): Promise<ConnectionStatus> {
    try {
      const capabilities =
        await this.request<NativeCapabilities>("get_capabilities");
      // A host outside our protocol window must fail loudly here — its
      // events would otherwise be dropped silently by isNativeEvent and the
      // extension would just look broken.
      if (!protocolCompatible(capabilities)) {
        this.setStatus({
          hostAvailable: true,
          backendConnected: false,
          capabilities,
          error: {
            code: "PROTOCOL_MISMATCH",
            message: `Ravyn speaks native protocol ${capabilities.protocolVersion} but this extension supports ${NATIVE_PROTOCOL_MIN}–${NATIVE_PROTOCOL_VERSION}. Update Ravyn or the extension.`,
            retryable: false,
          },
        });
        return this.statusValue;
      }
      this.setStatus({
        hostAvailable: true,
        backendConnected: capabilities.backendConnected,
        capabilities,
      });
    } catch (error) {
      const extensionError = toExtensionError(error, "NATIVE_STATUS_FAILED");
      this.setStatus({
        hostAvailable:
          extensionError.code !== "NATIVE_HOST_UNAVAILABLE" &&
          extensionError.code !== "NATIVE_CONNECT_FAILED",
        backendConnected: false,
        error: extensionError.toNativeError(),
      });
    }
    return this.statusValue;
  }

  private async openPort(): Promise<void> {
    try {
      const port = browser.runtime.connectNative(NATIVE_HOST_NAME);
      this.port = port;
      port.onMessage.addListener((message: unknown) =>
        this.handleMessage(message),
      );
      port.onDisconnect.addListener(() => this.handleDisconnect());
      await this.refreshStatus();
      // Only a confirmed-healthy connection resets the backoff. Firefox's
      // connectNative doesn't throw for a missing/crashing host — the port
      // opens and onDisconnect fires moments later — so resetting right
      // after connectNative would pin every retry at the shortest delay,
      // re-spawning the host process 4x/second forever instead of backing
      // off toward the 30s ceiling.
      if (this.statusValue.hostAvailable) this.reconnectAttempt = 0;
      void this.request("subscribe_events").catch((error) =>
        logger.warn("Backend event subscription is unavailable", error),
      );
      this.startHeartbeat();
    } catch (error) {
      this.port = null;
      const extensionError = toExtensionError(error, "NATIVE_CONNECT_FAILED");
      this.setStatus({
        hostAvailable: false,
        backendConnected: false,
        error: extensionError.toNativeError(),
      });
      this.scheduleReconnect();
      throw extensionError;
    }
  }

  private handleMessage(message: unknown): void {
    if (isNativeEvent(message)) {
      for (const listener of this.eventListeners) listener(message);
      if (
        message.event === "backend.connected" ||
        message.event === "backend.disconnected"
      )
        void this.refreshStatus();
      return;
    }
    if (!isNativeResponse(message)) {
      logger.warn("Ignored malformed native message", message);
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) return;
    window.clearTimeout(pending.timer);
    this.pending.delete(message.id);
    if (message.ok) pending.resolve(message.result);
    else
      pending.reject(
        new RavynExtensionError(
          message.error?.code ?? "NATIVE_ERROR",
          message.error?.message ?? "Ravyn rejected the request.",
          message.error?.retryable ?? false,
        ),
      );
  }

  private handleDisconnect(): void {
    const runtimeError = browser.runtime.lastError;
    this.stopHeartbeat();
    this.port = null;
    this.rejectPending(
      new RavynExtensionError(
        "NATIVE_DISCONNECTED",
        runtimeError?.message ?? "The Ravyn native connection closed.",
        true,
      ),
    );
    this.setStatus({
      hostAvailable: false,
      backendConnected: false,
      error: {
        code: "NATIVE_DISCONNECTED",
        message: runtimeError?.message ?? "The Ravyn native connection closed.",
        retryable: true,
      },
    });
    this.scheduleReconnect();
  }

  private startHeartbeat(): void {
    this.stopHeartbeat();
    this.heartbeatTimer = window.setInterval(() => {
      void this.refreshStatus();
    }, HEARTBEAT_INTERVAL_MS);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) window.clearInterval(this.heartbeatTimer);
    this.heartbeatTimer = null;
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) return;
    const delay =
      RECONNECT_DELAYS_MS[
        Math.min(this.reconnectAttempt, RECONNECT_DELAYS_MS.length - 1)
      ] ?? 30_000;
    this.reconnectAttempt += 1;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      void this.connect().catch(() => undefined);
    }, delay);
  }

  private rejectPending(error: RavynExtensionError): void {
    for (const pending of this.pending.values()) {
      window.clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
  }

  private setStatus(status: ConnectionStatus): void {
    this.statusValue = status;
    for (const listener of this.statusListeners) listener(status);
  }
}

function isNativeResponse(value: unknown): value is NativeResponse {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.id === "string" && typeof record.ok === "boolean";
}

function isNativeEvent(value: unknown): value is NativeEvent {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  const version = record.protocolVersion;
  // Accept the negotiated window rather than one exact version; an
  // out-of-window host is surfaced as PROTOCOL_MISMATCH by refreshStatus,
  // so dropping its events here is a backstop, not the user-facing signal.
  return (
    record.type === "event" &&
    typeof record.event === "string" &&
    typeof version === "number" &&
    version >= NATIVE_PROTOCOL_MIN &&
    version <= NATIVE_PROTOCOL_VERSION
  );
}
