import type { BackgroundRequest } from "../shared/contracts";

const params = new URLSearchParams(location.search);
const requestId = params.get("id") ?? "";
element("filename").textContent = params.get("filename") ?? "Download";
element("url").textContent = params.get("url") ?? "";
element("send").addEventListener("click", () => void decide(true));
element("continue").addEventListener("click", () => void decide(false));
window.addEventListener("beforeunload", () => {
  if (requestId)
    void browser.runtime.sendMessage({
      type: "confirmation-result",
      requestId,
      accepted: false,
    } satisfies BackgroundRequest);
});

async function decide(accepted: boolean): Promise<void> {
  if (requestId)
    await browser.runtime.sendMessage({
      type: "confirmation-result",
      requestId,
      accepted,
    } satisfies BackgroundRequest);
  window.close();
}

function element<T extends HTMLElement = HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing element #${id}`);
  return value as T;
}
