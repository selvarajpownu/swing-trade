import type { LucideIcon } from "lucide-react";

import { Card } from "./card";

interface ComingSoonProps {
  icon: LucideIcon;
  title: string;
  description: string;
  phase?: string;
}

export function ComingSoon({ icon: Icon, title, description, phase }: ComingSoonProps) {
  return (
    <Card className="flex flex-col items-center px-6 py-16 text-center">
      <span className="flex size-12 items-center justify-center rounded-2xl bg-surface-3">
        <Icon aria-hidden="true" className="size-6 text-muted" />
      </span>
      <h1 className="mt-4 text-lg font-semibold">{title}</h1>
      <p className="mt-1 max-w-md text-sm text-muted">{description}</p>
      <p className="mt-4 rounded-full border border-line px-3 py-1 text-xs text-subtle">
        {phase ?? "Coming soon"}
      </p>
    </Card>
  );
}
