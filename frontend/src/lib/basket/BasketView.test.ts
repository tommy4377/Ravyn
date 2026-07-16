// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RavynClient } from "../api/client";
import type { BasketItem } from "../api/types";
import { connection } from "../stores/connection.svelte";
import BasketView from "./BasketView.svelte";

const item: BasketItem = {
  id: "basket-1",
  position: 0,
  request: {
    kind: "http",
    source: "https://example.com/file.zip",
    destination: "C:\\Downloads",
    options: {},
  },
  preset_id: null,
  created_at: "2026-07-16T12:00:00Z",
  updated_at: "2026-07-16T12:00:00Z",
};

afterEach(() => {
  connection.client = null;
  cleanup();
});

describe("BasketView", () => {
  it("keeps start and clear actions visible in the drawer content", async () => {
    const startBasket = vi.fn().mockResolvedValue({
      items: [],
      started: 1,
      failed: 0,
    });
    const listBasket = vi
      .fn()
      .mockResolvedValueOnce([item])
      .mockResolvedValueOnce([]);
    connection.client = {
      listBasket,
      startBasket,
    } as unknown as RavynClient;

    const { getByRole } = render(BasketView, { props: { embedded: true } });
    const start = await waitFor(() => getByRole("button", { name: "Start 1" }));
    expect(getByRole("button", { name: "Clear" })).not.toBeNull();
    expect(getByRole("button", { name: "Add" })).not.toBeNull();

    await fireEvent.click(start);
    await waitFor(() => expect(startBasket).toHaveBeenCalledOnce());
    await waitFor(() => expect(listBasket).toHaveBeenCalledTimes(2));
  });
});
