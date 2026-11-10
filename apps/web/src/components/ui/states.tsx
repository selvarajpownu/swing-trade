import type { ReactNode } from "react";
import { Inbox, TriangleAlert } from "lucide-react";

import { cn } from "@/lib/utils";

interface StateProps {
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
}

export function EmptyState({ title, description, action, className }: StateProps) {
  return (
    <div className={cn("flex flex-col items-center px-6 py-14 text-center", className)}>
      <Inbox aria-hidden="true" className="size-8 text-subtle" />
      <p className="mt-3 text-sm font-medium text-fg">{title}</p>
      {description ? <p className="mt-1 max-w-sm text-sm text-muted">{description}</p> : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}

export function ErrorState({ title, description, action, className }: StateProps) {
  return (
    <div
      role="alert"
      className={cn("flex flex-col items-center px-6 py-14 text-center", className)}
    >
      <TriangleAlert aria-hidden="true" className="size-8 text-down" />
      <p className="mt-3 text-sm font-medium text-fg">{title}</p>
      {description ? <p className="mt-1 max-w-md text-sm text-muted">{description}</p> : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
