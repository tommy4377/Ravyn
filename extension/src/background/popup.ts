import type { ResourceKind } from "../shared/contracts";

export async function openResourcePopup(
  type: ResourceKind | "all" = "all",
): Promise<void> {
  await browser.storage.local.set({
    "ravyn.popupView": "resources",
    "ravyn.popupType": type,
  });
  await browser.action.openPopup();
}
