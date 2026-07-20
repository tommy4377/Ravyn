import type { Page } from "./types";

const DEFAULT_PAGE_SIZE = 200;
const DEFAULT_MAX_ITEMS = 50_000;

/** Collects a cursor-paginated endpoint without silently truncating at one page. */
export async function collectAllPages<T>(
  fetchPage: (cursor: string | undefined, limit: number) => Promise<Page<T>>,
  options: { pageSize?: number; maxItems?: number } = {},
): Promise<T[]> {
  const pageSize = Math.min(200, Math.max(1, options.pageSize ?? DEFAULT_PAGE_SIZE));
  const maxItems = Math.max(pageSize, options.maxItems ?? DEFAULT_MAX_ITEMS);
  const items: T[] = [];
  let cursor: string | undefined;
  const seen = new Set<string>();

  do {
    const page = await fetchPage(cursor, pageSize);
    items.push(...page.items);
    if (items.length > maxItems)
      throw new Error(`Paginated response exceeded the ${maxItems}-item safety limit.`);
    const next = page.next_cursor ?? undefined;
    if (next && seen.has(next)) throw new Error("Paginated endpoint returned a repeated cursor.");
    if (next) seen.add(next);
    cursor = next;
  } while (cursor);

  return items;
}
