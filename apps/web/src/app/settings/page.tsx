import type { Metadata } from "next";
import type { ReactNode } from "react";
import { CircleCheck, CircleX, Sparkles } from "lucide-react";

import { describeApiError } from "@/lib/api/errors";
import { apiBaseUrl, getHealth, isApiTokenConfigured } from "@/lib/api/server";
import type { HealthResponse } from "@/lib/api/types";
import { APP_NAME } from "@/lib/utils";

import { Badge } from "@/components/ui/badge";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { RetryButton } from "@/components/ui/retry-button";

export const metadata: Metadata = { title: "Settings" };

export const dynamic = "force-dynamic";

const ENV_VARS = [
  {
    name: "API_URL",
    scope: "Server",
    description: "Base URL of the Rust API. Defaults to http://localhost:8080.",
  },
  {
    name: "API_TOKEN",
    scope: "Server",
    description:
      "Bearer token sent to the API on every /v1 call. Read only on the server; never sent to the browser.",
  },
  {
    name: "NEXT_PUBLIC_APP_NAME",
    scope: "Build time",
    description:
      "Name shown in the sidebar and page titles. Inlined into the browser bundle at build.",
  },
] as const;

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <dt className="text-sm text-muted">{label}</dt>
      <dd className="min-w-0 truncate text-right text-sm text-fg">{children}</dd>
    </div>
  );
}

export default async function SettingsPage() {
  let health: HealthResponse | null = null;
  let healthError: string | null = null;
  try {
    health = await getHealth();
  } catch (err) {
    healthError = describeApiError(err);
  }
  const tokenSet = isApiTokenConfigured();
  const healthy = health?.status === "ok";

  return (
    <>
      <PageHeader
        title="Settings"
        description={`Read-only view of how ${APP_NAME} is configured. Change settings through environment variables and restart.`}
      />

      <div className="grid gap-4 lg:grid-cols-2">
        <Card aria-labelledby="backend-title">
          <CardHeader
            title="Backend"
            titleId="backend-title"
            description="Live result of GET /health on the API."
            action={
              healthy ? (
                <Badge tone="up">
                  <CircleCheck aria-hidden="true" className="size-3" /> Online
                </Badge>
              ) : (
                <Badge tone="down">
                  <CircleX aria-hidden="true" className="size-3" />{" "}
                  {health ? health.status : "Unreachable"}
                </Badge>
              )
            }
          />
          <CardBody>
            <dl className="divide-y divide-line">
              <Row label="API URL">
                <code className="font-mono text-xs">{apiBaseUrl()}</code>
              </Row>
              <Row label="API token">
                {tokenSet ? "Configured" : <span className="text-warn">Not set</span>}
              </Row>
              <Row label="API version">{health?.version ?? "—"}</Row>
              <Row label="Status">{health?.status ?? "—"}</Row>
            </dl>
            {healthError ? (
              <div
                role="alert"
                className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-down/30 bg-down/10 px-3 py-2 text-sm text-fg"
              >
                <span>{healthError}</span>
                <RetryButton label="Check again" />
              </div>
            ) : null}
          </CardBody>
        </Card>

        <Card aria-labelledby="providers-title">
          <CardHeader
            title="Data providers"
            titleId="providers-title"
            description="Market data sources the API has enabled."
          />
          <CardBody>
            {health && health.providers.length > 0 ? (
              <ul className="flex flex-wrap gap-2">
                {health.providers.map((provider) => (
                  <li key={provider}>
                    <Badge
                      tone={provider === "demo" ? "warn" : "accent"}
                      className="px-2.5 py-1 text-xs"
                    >
                      {provider}
                    </Badge>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-sm text-muted">
                {health ? "No providers reported." : "Unknown while the API is unreachable."}
              </p>
            )}
            <p className="mt-4 text-sm leading-6 text-muted">
              Providers are configured on the API service (
              <code className="font-mono text-xs">services/api</code>), not here. The web app never
              talks to a broker or data vendor directly; it only reads from the API. When the API
              answers from the <strong className="text-fg">demo</strong> provider, a{" "}
              <span className="font-semibold text-warn">DEMO DATA</span> pill shows in the top bar.
            </p>
          </CardBody>
        </Card>

        <Card aria-labelledby="ai-title">
          <CardHeader
            title="AI providers"
            titleId="ai-title"
            description="News and filing summaries."
            action={<Badge tone="muted">Phase 4</Badge>}
          />
          <CardBody>
            <div className="flex gap-3 text-sm leading-6 text-muted">
              <Sparkles aria-hidden="true" className="mt-1 size-4 shrink-0 text-accent" />
              <p>
                AI summaries are coming later. They will be configured on the server (hosted model
                API keys or a local model), so keys never reach the browser.
              </p>
            </div>
          </CardBody>
        </Card>

        <Card aria-labelledby="env-title">
          <CardHeader
            title="Web app environment"
            titleId="env-title"
            description="Set these in .env.local (development) or the container environment."
          />
          <CardBody>
            <dl className="space-y-3">
              {ENV_VARS.map((v) => (
                <div key={v.name}>
                  <dt className="flex items-center gap-2">
                    <code className="font-mono text-xs text-fg">{v.name}</code>
                    <Badge tone="muted">{v.scope}</Badge>
                  </dt>
                  <dd className="mt-1 text-sm text-muted">{v.description}</dd>
                </div>
              ))}
            </dl>
          </CardBody>
        </Card>
      </div>
    </>
  );
}
