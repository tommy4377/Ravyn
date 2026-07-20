import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, SETTINGS_KEY } from "../../shared/settings";
import { BypassRegistry } from "./bypass";
import { DelegationRegistry } from "./delegation";
import { DownloadInterceptor } from "./interceptor";
import type { RuleCache } from "../rules/cache";
import type { NativeClient } from "../native/client";

type StoredSettings = Partial<typeof DEFAULT_SETTINGS>;

function makeInterceptor(
  overrides: StoredSettings = {},
  ruleAction: "ravyn" | "ask" | "browser" | "ignore" | undefined = undefined,
) {
  const stored: Record<string, unknown> = {
    [SETTINGS_KEY]: { ...DEFAULT_SETTINGS, ...overrides },
  };
  const pause = vi.fn().mockResolvedValue(undefined);
  const resume = vi.fn().mockResolvedValue(undefined);
  const cancel = vi.fn().mockResolvedValue(undefined);
  const removeFile = vi.fn().mockResolvedValue(undefined);
  const erase = vi.fn().mockResolvedValue(undefined);
  const search = vi.fn().mockResolvedValue([]);
  const notificationsCreate = vi.fn().mockResolvedValue("note-id");
  const windowsCreate = vi.fn().mockResolvedValue({});
  vi.stubGlobal("browser", {
    downloads: { pause, resume, cancel, removeFile, erase, search },
    storage: {
      local: {
        get: vi.fn().mockImplementation(() => Promise.resolve(stored)),
        set: vi.fn().mockImplementation((patch: Record<string, unknown>) => {
          Object.assign(stored, patch);
          return Promise.resolve();
        }),
      },
    },
    runtime: {
      id: "ravyn@test",
      getURL: (path: string) => `moz-ext://${path}`,
    },
    notifications: { create: notificationsCreate },
    windows: { create: windowsCreate },
  });

  const request = vi.fn().mockResolvedValue({ id: "job-1" });
  const native = { request } as unknown as NativeClient;
  const rules = {
    get: vi.fn().mockResolvedValue(
      ruleAction
        ? [
            {
              id: "r1",
              name: "rule",
              priority: 0,
              enabled: true,
              domains: [],
              extensions: [],
              mimePatterns: [],
              action: ruleAction,
            },
          ]
        : [],
    ),
  } as unknown as RuleCache;
  const delegated = new DelegationRegistry();
  const bypass = new BypassRegistry();
  const interceptor = new DownloadInterceptor(native, rules, delegated, bypass);
  return {
    interceptor,
    bypass,
    pause,
    resume,
    cancel,
    removeFile,
    request,
    notificationsCreate,
    windowsCreate,
  };
}

function downloadItem(
  overrides: Partial<browser.downloads.DownloadItem> = {},
): browser.downloads.DownloadItem {
  return {
    id: 42,
    url: "https://example.com/file.zip",
    filename: "file.zip",
    incognito: false,
    paused: false,
    canResume: true,
    danger: "safe",
    exists: true,
    startTime: "2026-01-01T00:00:00.000Z",
    state: "in_progress",
    totalBytes: 1024,
    bytesReceived: 0,
    fileSize: -1,
    ...overrides,
  };
}

function confirmationIdFromCall(
  windowsCreate: ReturnType<typeof vi.fn>,
  callIndex: number,
): string {
  const call = windowsCreate.mock.calls[callIndex] as
    [{ url: string }] | undefined;
  if (!call)
    throw new Error(`windows.create was not called ${callIndex + 1} time(s)`);
  const url = new URL(call[0].url.replace("moz-ext://", "https://x/"));
  const id = url.searchParams.get("id");
  if (!id) throw new Error("confirmation URL had no id parameter");
  return id;
}

async function handle(
  interceptor: DownloadInterceptor,
  item: browser.downloads.DownloadItem,
): Promise<void> {
  // handle() is private; onCreated is the only public entry point, and it
  // swallows rejections by design (matching production wiring in
  // background/index.ts), so failures surface as "the expected calls never
  // happened" rather than a thrown test error.
  await (
    interceptor as unknown as {
      handle(item: browser.downloads.DownloadItem): Promise<void>;
    }
  ).handle(item);
}

describe("DownloadInterceptor.handle", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("leaves a bypassed download untouched", async () => {
    const { interceptor, bypass, pause, request } = makeInterceptor();
    await bypass.arm("https://example.com/file.zip");
    await handle(interceptor, downloadItem());
    expect(pause).not.toHaveBeenCalled();
    expect(request).not.toHaveBeenCalled();
  });

  it("leaves the browser download untouched when interception is disabled", async () => {
    const { interceptor, pause, resume, request } = makeInterceptor({
      automaticInterception: false,
    });
    await handle(interceptor, downloadItem());
    expect(pause).not.toHaveBeenCalled();
    expect(resume).not.toHaveBeenCalled();
    expect(request).not.toHaveBeenCalled();
  });

  it("resumes without handing off when a rule says ignore", async () => {
    const { interceptor, pause, resume, request } = makeInterceptor(
      {},
      "ignore",
    );
    await handle(interceptor, downloadItem());
    expect(pause).toHaveBeenCalledWith(42);
    expect(resume).toHaveBeenCalledWith(42);
    expect(request).not.toHaveBeenCalled();
  });

  it("hands an eligible download to Ravyn and cancels the browser copy", async () => {
    const { interceptor, pause, resume, cancel, removeFile, request } =
      makeInterceptor();
    await handle(interceptor, downloadItem());
    expect(pause).toHaveBeenCalledWith(42);
    expect(request).toHaveBeenCalledWith(
      "create_download",
      expect.objectContaining({ url: "https://example.com/file.zip" }),
    );
    expect(cancel).toHaveBeenCalledWith(42);
    expect(removeFile).toHaveBeenCalledWith(42);
    // Handed off — must not also resume the (now cancelled) browser item.
    expect(resume).not.toHaveBeenCalled();
  });

  it("hands off one concrete Firefox download only once when the same event races", async () => {
    // The same browser download event may be delivered twice while the first
    // asynchronous handoff is still in flight. The browser download id, not
    // the URL, is the ownership key.
    const { interceptor, cancel, resume, request } = makeInterceptor();
    let releaseRequest!: () => void;
    request.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseRequest = () => resolve({ id: "job-1" });
        }),
    );
    const first = handle(interceptor, downloadItem({ id: 42 }));
    const second = handle(interceptor, downloadItem({ id: 42 }));
    await vi.waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(resume).toHaveBeenCalledWith(42));
    releaseRequest();
    await Promise.all([first, second]);
    expect(request).toHaveBeenCalledTimes(1);
    expect(cancel).toHaveBeenCalledExactlyOnceWith(42);
  });

  it("allows distinct Firefox downloads of the same URL to hand off independently", async () => {
    const { interceptor, cancel, request } = makeInterceptor();
    await Promise.all([
      handle(interceptor, downloadItem({ id: 42 })),
      handle(interceptor, downloadItem({ id: 43 })),
    ]);
    expect(request).toHaveBeenCalledTimes(2);
    expect(cancel).toHaveBeenCalledWith(42);
    expect(cancel).toHaveBeenCalledWith(43);
  });

  it("resumes the browser download and notifies when the native handoff fails", async () => {
    const { interceptor, resume, notificationsCreate, request } =
      makeInterceptor();
    request.mockRejectedValueOnce(new Error("native host unavailable"));
    await handle(interceptor, downloadItem());
    expect(resume).toHaveBeenCalledWith(42);
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Ravyn handoff failed" }),
    );
  });

  it("does not hand off a declined confirmation, and resumes it", async () => {
    const { interceptor, resume, request, windowsCreate } = makeInterceptor(
      {},
      "ask",
    );
    const pending = handle(interceptor, downloadItem());
    await vi.waitFor(() => expect(windowsCreate).toHaveBeenCalled());
    const requestId = confirmationIdFromCall(windowsCreate, 0);
    interceptor.resolveConfirmation(requestId, false);
    await pending;
    expect(request).not.toHaveBeenCalled();
    expect(resume).toHaveBeenCalledWith(42);
  });

  it("hands off an accepted confirmation", async () => {
    const { interceptor, request, windowsCreate } = makeInterceptor({}, "ask");
    const pending = handle(interceptor, downloadItem());
    await vi.waitFor(() => expect(windowsCreate).toHaveBeenCalled());
    const requestId = confirmationIdFromCall(windowsCreate, 0);
    interceptor.resolveConfirmation(requestId, true);
    await pending;
    expect(request).toHaveBeenCalledWith("create_download", expect.anything());
  });

  it("serializes two simultaneous confirmations into one window at a time", async () => {
    const { interceptor, windowsCreate } = makeInterceptor({}, "ask");
    const first = handle(
      interceptor,
      downloadItem({ id: 1, url: "https://example.com/a.zip" }),
    );
    const second = handle(
      interceptor,
      downloadItem({ id: 2, url: "https://example.com/b.zip" }),
    );
    await vi.waitFor(() => expect(windowsCreate).toHaveBeenCalledTimes(1));
    interceptor.resolveConfirmation(
      confirmationIdFromCall(windowsCreate, 0),
      false,
    );
    await first;
    await vi.waitFor(() => expect(windowsCreate).toHaveBeenCalledTimes(2));
    interceptor.resolveConfirmation(
      confirmationIdFromCall(windowsCreate, 1),
      false,
    );
    await second;
  });
});
