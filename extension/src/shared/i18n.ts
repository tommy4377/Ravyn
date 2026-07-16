export function message(
  name: string,
  substitutions?: string | string[],
): string {
  return browser.i18n.getMessage(name, substitutions) || name;
}
