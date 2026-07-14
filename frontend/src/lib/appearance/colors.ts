export interface AccentPalette {
  default: string;
  hover: string;
  pressed: string;
  text: string;
  onColor: string;
  subtle: string;
  border: string;
}

type Rgb = { r: number; g: number; b: number };
type Hsl = { h: number; s: number; l: number };

const FALLBACK_ACCENT = "#0067c0";

export function normalizeHexColor(value: string | null | undefined): string | null {
  const input = value?.trim().toLowerCase();
  if (!input) return null;
  if (/^#[0-9a-f]{6}$/.test(input)) return input;
  if (/^#[0-9a-f]{3}$/.test(input)) {
    return `#${input[1]}${input[1]}${input[2]}${input[2]}${input[3]}${input[3]}`;
  }
  return null;
}

export function createAccentPalette(
  source: string | null | undefined,
  theme: "light" | "dark",
): AccentPalette {
  const normalized = normalizeHexColor(source) ?? FALLBACK_ACCENT;
  const hsl = rgbToHsl(hexToRgb(normalized));
  const base = hslToRgb({
    h: hsl.h,
    s: clamp(hsl.s, 0.52, 0.92),
    l: theme === "dark" ? clamp(hsl.l, 0.62, 0.72) : clamp(hsl.l, 0.34, 0.47),
  });
  const hover = shiftLightness(base, theme === "dark" ? 0.055 : -0.045);
  const pressed = shiftLightness(base, theme === "dark" ? -0.075 : -0.105);
  const text = shiftLightness(base, theme === "dark" ? 0.08 : -0.02);
  const onColor = contrastRatio(base, { r: 255, g: 255, b: 255 })
    >= contrastRatio(base, { r: 5, g: 15, b: 22 })
    ? "#ffffff"
    : "#050f16";

  return {
    default: rgbToHex(base),
    hover: rgbToHex(hover),
    pressed: rgbToHex(pressed),
    text: rgbToHex(text),
    onColor,
    subtle: rgbToRgba(base, theme === "dark" ? 0.14 : 0.11),
    border: rgbToRgba(base, theme === "dark" ? 0.42 : 0.36),
  };
}

function hexToRgb(value: string): Rgb {
  return {
    r: Number.parseInt(value.slice(1, 3), 16),
    g: Number.parseInt(value.slice(3, 5), 16),
    b: Number.parseInt(value.slice(5, 7), 16),
  };
}

function rgbToHex({ r, g, b }: Rgb): string {
  const channel = (value: number): string => Math.round(clamp(value, 0, 255)).toString(16).padStart(2, "0");
  return `#${channel(r)}${channel(g)}${channel(b)}`;
}

function rgbToRgba({ r, g, b }: Rgb, alpha: number): string {
  return `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${alpha})`;
}

function rgbToHsl({ r, g, b }: Rgb): Hsl {
  const red = r / 255;
  const green = g / 255;
  const blue = b / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const delta = max - min;
  const l = (max + min) / 2;
  if (delta === 0) return { h: 0, s: 0, l };

  const s = delta / (1 - Math.abs(2 * l - 1));
  let h = 0;
  if (max === red) h = 60 * (((green - blue) / delta) % 6);
  else if (max === green) h = 60 * ((blue - red) / delta + 2);
  else h = 60 * ((red - green) / delta + 4);
  if (h < 0) h += 360;
  return { h, s, l };
}

function hslToRgb({ h, s, l }: Hsl): Rgb {
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const section = h / 60;
  const x = chroma * (1 - Math.abs((section % 2) - 1));
  let red = 0;
  let green = 0;
  let blue = 0;
  if (section < 1) [red, green] = [chroma, x];
  else if (section < 2) [red, green] = [x, chroma];
  else if (section < 3) [green, blue] = [chroma, x];
  else if (section < 4) [green, blue] = [x, chroma];
  else if (section < 5) [red, blue] = [x, chroma];
  else [red, blue] = [chroma, x];
  const match = l - chroma / 2;
  return { r: (red + match) * 255, g: (green + match) * 255, b: (blue + match) * 255 };
}

function shiftLightness(rgb: Rgb, shift: number): Rgb {
  const hsl = rgbToHsl(rgb);
  return hslToRgb({ ...hsl, l: clamp(hsl.l + shift, 0.08, 0.92) });
}

function relativeLuminance({ r, g, b }: Rgb): number {
  const channel = (value: number): number => {
    const normalized = value / 255;
    return normalized <= 0.04045
      ? normalized / 12.92
      : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrastRatio(left: Rgb, right: Rgb): number {
  const high = Math.max(relativeLuminance(left), relativeLuminance(right));
  const low = Math.min(relativeLuminance(left), relativeLuminance(right));
  return (high + 0.05) / (low + 0.05);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
