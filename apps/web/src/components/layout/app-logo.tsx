import { cn } from "@/lib/utils";

/** Simple mark: a rising swing line in a rounded square. */
export function AppLogo({ className }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      className={cn("size-7 shrink-0", className)}
      fill="none"
    >
      <rect width="24" height="24" rx="7" className="fill-accent/15" />
      <path
        d="M5 15.5l3.5-3.5 3 2.5L19 7"
        className="stroke-accent"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="19" cy="7" r="1.6" className="fill-up" />
    </svg>
  );
}
