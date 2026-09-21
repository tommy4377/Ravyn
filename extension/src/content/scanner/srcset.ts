export interface SrcsetCandidate {
  url: string;
  descriptor?: string;
}

export function parseSrcset(value: string): SrcsetCandidate[] {
  const candidates: SrcsetCandidate[] = [];
  let buffer = "";
  let parenDepth = 0;
  const flush = (): void => {
    const candidate = buffer.trim();
    buffer = "";
    if (!candidate) return;
    const match = /^(\S+)(?:\s+(.+))?$/.exec(candidate);
    if (match?.[1])
      candidates.push({
        url: match[1],
        descriptor: match[2]?.trim() || undefined,
      });
  };
  for (const character of value) {
    if (character === "(") parenDepth += 1;
    if (character === ")" && parenDepth > 0) parenDepth -= 1;
    if (character === "," && parenDepth === 0) flush();
    else buffer += character;
  }
  flush();
  return candidates;
}
