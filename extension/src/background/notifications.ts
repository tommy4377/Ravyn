export async function notify(title: string, message: string): Promise<void> {
  await browser.notifications.create({
    type: "basic",
    iconUrl: browser.runtime.getURL("icons/ravyn-96.png"),
    title,
    message,
  });
}
