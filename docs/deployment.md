# Deployment

Goal: run it for free first, move to a small paid server only when needed.

```
Browser ──> Vercel (apps/web) ──HTTPS + Bearer token──> API host (services/api)
```

The web app on Vercel must reach the API over **HTTPS** with the `API_TOKEN`. The API itself never needs to be reachable from browsers.

## Free setup

### Web: Vercel Hobby

1. Import the GitHub repo in Vercel. Set **Root Directory** to `apps/web` (framework: Next.js, detected automatically).
2. Environment variables (Production and Preview):
   - `API_URL` = the public HTTPS URL of your API, e.g. `https://api.example.com`
   - `API_TOKEN` = the same value as on the API
   - `NEXT_PUBLIC_APP_NAME` = `Swing Trade`
3. Deploy. Vercel ignores the Dockerfile; it builds Next.js natively.

Hobby is for personal, non-commercial use, which matches this app.

### API option A: Oracle Cloud Always Free ARM VM

An Ampere A1 VM with 2 OCPU / 12 GB RAM fits comfortably in the Always Free allowance.

1. Create the VM (Ubuntu 24.04, ARM). In the VCN security list open TCP 80 and 443 only. Do **not** open 8080.
2. Install Docker Engine and the Compose plugin.
3. Clone the repo, create `.env` (`LOG_FORMAT=json`, a strong `API_TOKEN`), and `mkdir -p .data && sudo chown 10001 .data`.
4. Start only the API: `docker compose up -d --build api`. The images are multi-arch, so it builds natively on ARM.
5. Put HTTPS in front, either:
   - Caddy on the VM (`api.example.com { reverse_proxy localhost:8080 }`, automatic Let's Encrypt), or
   - Cloudflare Tunnel (see option B) and keep 80/443 closed too.
6. In `docker-compose.yml` you can bind the port to localhost only: `"127.0.0.1:8080:8080"`.

### API option B: your own laptop + Cloudflare Tunnel

Good for trying things out; the API is only up while the laptop is on.

1. Run the API (`cargo run -p api-server -- serve` or `docker compose up -d api`).
2. Install `cloudflared` and log in: `cloudflared tunnel login`.
3. Create a tunnel and route a hostname to it:
   ```bash
   cloudflared tunnel create swing-api
   cloudflared tunnel route dns swing-api api.example.com
   cloudflared tunnel run --url http://localhost:8080 swing-api
   ```
4. Use `https://api.example.com` as `API_URL` on Vercel. No router ports are opened.

A free Cloudflare account and a domain on Cloudflare DNS are needed for a named tunnel.

### INDmoney token on a server

`swing-api login indmoney` needs a browser. Log in on your laptop, then copy the token file to the server's `.data/indmoney-token.json` (for example with `scp`), or set `INDMONEY_ACCESS_TOKEN`. Treat the file like a password. If you don't want INDmoney on the server, use `DATA_PROVIDERS=demo`.

## Paid later: AWS Lightsail (Mumbai)

When you want an always-on box close to the exchange and to host both parts together:

- Lightsail instance in `ap-south-1` (Mumbai), roughly **$12-24/month** (2-4 GB RAM).
- Run the full stack with `docker compose up -d --build` (api + web, and Postgres/TimescaleDB from phase 3).
- Caddy in front for HTTPS:
  ```
  swing.example.com {
    reverse_proxy localhost:3000
  }
  ```
  The API stays private on the Docker network; only the web container calls it.
- Open only 80/443 in the Lightsail firewall.

## Checklist

- [ ] `API_TOKEN` is long and random, and identical on web and API.
- [ ] `ALLOW_NO_TOKEN` is `false`.
- [ ] Port 8080 is not open to the internet; HTTPS only.
- [ ] `.env` and `.data/` are not in git and not world-readable on the server.
- [ ] `curl https://<api-host>/health` answers; `curl https://<api-host>/v1/stocks` without a token returns 401.
