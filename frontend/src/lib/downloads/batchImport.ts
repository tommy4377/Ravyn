import type { CreateJob } from "../api/types";

export interface BatchInputSummary {
  lines: string[];
  uniqueLines: string[];
  duplicateCount: number;
  jsonBatch: CreateJob[] | null;
  itemCount: number;
}

export function analyzeBatchInput(text: string): BatchInputSummary {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#") && !line.startsWith("//"));
  const uniqueLines = [...new Set(lines)];
  const jsonBatch = parseJobBatch(text);
  return {
    lines,
    uniqueLines,
    duplicateCount: Math.max(0, lines.length - uniqueLines.length),
    jsonBatch,
    itemCount: jsonBatch?.length ?? uniqueLines.length,
  };
}

function parseJobBatch(text: string): CreateJob[] | null {
  const value = text.trim();
  if (!value.startsWith("[")) return null;
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed) || !parsed.every(isCreateJobLike)) return null;
    return parsed as CreateJob[];
  } catch {
    return null;
  }
}

function isCreateJobLike(value: unknown): boolean {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<CreateJob>;
  return typeof candidate.source === "string" && candidate.source.trim().length > 0;
}
