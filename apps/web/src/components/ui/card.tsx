import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "@/lib/utils";

export function Card({ className, ...props }: ComponentPropsWithoutRef<"section">) {
  return (
    <section
      className={cn(
        "rounded-2xl border border-line bg-surface shadow-sm shadow-black/20",
        className,
      )}
      {...props}
    />
  );
}

interface CardHeaderProps {
  title: ReactNode;
  /** id for the heading, so the card can use aria-labelledby. */
  titleId?: string;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
}

export function CardHeader({ title, titleId, description, action, className }: CardHeaderProps) {
  return (
    <header className={cn("flex items-start justify-between gap-3 px-4 pt-4 pb-3", className)}>
      <div className="min-w-0">
        <h2 id={titleId} className="text-sm font-semibold text-fg">
          {title}
        </h2>
        {description ? <p className="mt-0.5 text-xs text-muted">{description}</p> : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </header>
  );
}

export function CardBody({ className, ...props }: ComponentPropsWithoutRef<"div">) {
  return <div className={cn("px-4 pb-4", className)} {...props} />;
}
