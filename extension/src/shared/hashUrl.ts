import { urlHashInput } from "./urls";

export async function hashUrl(url: string): Promise<string> {
  const bytes = new TextEncoder().encode(urlHashInput(url));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
}
