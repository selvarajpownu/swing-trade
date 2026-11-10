import type { Metadata } from "next";
import { NotebookPen } from "lucide-react";

import { ComingSoon } from "@/components/ui/coming-soon";

export const metadata: Metadata = { title: "Journal" };

export default function JournalPage() {
  return (
    <ComingSoon
      icon={NotebookPen}
      title="Journal"
      description="Log the trades you take elsewhere, with your plan, outcome and notes, to review what works."
    />
  );
}
