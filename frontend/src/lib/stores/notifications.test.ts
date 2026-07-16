import { afterEach, describe, expect, it } from "vitest";
import { notifications } from "./notifications.svelte";

describe("notifications", () => {
  afterEach(() => notifications.clear());

  it("keeps the transient queue compact while preserving notification history", () => {
    for (let index = 0; index < 5; index += 1) {
      notifications.info(`Notice ${index}`);
    }

    expect(notifications.items).toHaveLength(4);
    expect(notifications.items.map((item) => item.title)).toEqual([
      "Notice 1",
      "Notice 2",
      "Notice 3",
      "Notice 4",
    ]);
    expect(notifications.history).toHaveLength(5);
  });
});
