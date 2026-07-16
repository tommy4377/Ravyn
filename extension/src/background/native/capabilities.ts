import type { NativeCapabilities } from "../../shared/contracts";

export function supports(
  capabilities: NativeCapabilities | undefined,
  feature: string,
): boolean {
  return capabilities?.features.includes(feature) ?? false;
}
