import { cn, symbolColor } from "@/lib/utils";

interface SymbolAvatarProps {
  symbol: string;
  name?: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const SIZES = {
  sm: "size-7 text-xs",
  md: "size-9 text-sm",
  lg: "size-12 text-lg",
} as const;

/** Round logo placeholder: first letter of the name on a colour derived from the symbol. */
export function SymbolAvatar({ symbol, name, size = "md", className }: SymbolAvatarProps) {
  const letter = (name?.trim() || symbol).charAt(0).toUpperCase();
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-full font-semibold text-white/95 ring-1 ring-white/10",
        SIZES[size],
        className,
      )}
      style={{ backgroundColor: symbolColor(symbol) }}
    >
      {letter}
    </span>
  );
}
