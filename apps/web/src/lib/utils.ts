import { clsx, type ClassValue } from "clsx";

/** Join class names conditionally. */
export function cn(...inputs: ClassValue[]): string {
  return clsx(inputs);
}

/** Small, stable string hash (FNV-1a, 32-bit). */
export function hashString(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** A deterministic background colour for a symbol's avatar. */
export function symbolColor(symbol: string): string {
  const hue = hashString(symbol) % 360;
  return `hsl(${hue} 55% 38%)`;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export const APP_NAME = process.env.NEXT_PUBLIC_APP_NAME?.trim() || "Swing Trade";
