# syntax=docker/dockerfile:1
# Requires `output: "standalone"` in next.config.
# Build:  docker build -t swing-web --build-arg NEXT_PUBLIC_APP_NAME="Swing Trade" apps/web
# Run:    docker run -e API_URL=http://host.docker.internal:8080 -e API_TOKEN=... -p 3000:3000 swing-web

# ---- deps ------------------------------------------------------------------
FROM node:22-alpine AS deps
WORKDIR /app
RUN apk add --no-cache libc6-compat
COPY package.json package-lock.json* ./
# npm ci when a lockfile exists, npm install otherwise.
RUN --mount=type=cache,target=/root/.npm \
    if [ -f package-lock.json ]; then npm ci; else npm install; fi

# ---- builder ---------------------------------------------------------------
FROM node:22-alpine AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .

# NEXT_PUBLIC_* values are inlined into the client bundle at build time.
ARG NEXT_PUBLIC_APP_NAME="Swing Trade"
ENV NEXT_PUBLIC_APP_NAME=$NEXT_PUBLIC_APP_NAME \
    NEXT_TELEMETRY_DISABLED=1

# public/ is optional in the source tree; make sure the runner COPY finds it.
RUN mkdir -p public && npm run build

# ---- runner ----------------------------------------------------------------
FROM node:22-alpine AS runner
WORKDIR /app

ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0

RUN addgroup -S -g 1001 nodejs && adduser -S -u 1001 -G nodejs nextjs

COPY --from=builder --chown=nextjs:nodejs /app/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/.next/static ./.next/static

USER nextjs
EXPOSE 3000

# API_URL and API_TOKEN are read at runtime on the server; pass them with -e or Compose.
CMD ["node", "server.js"]
