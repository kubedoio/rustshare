# RustShare Static Documentation Website

This directory contains the source code for the official, high-performance static documentation website for **RustShare** (modern file-sharing and company memory infrastructure). 

The website is hosted on **docs.rustshare.io** via **Cloudflare Workers** using modern Cloudflare Workers Assets. It is engineered with plain HTML5, custom utility-free HSL CSS variable themes, and lightweight client-side JavaScript—providing single-page hash routing, an offline search indexing modal, and code copy-to-clipboard buttons.

---

## ⚡ Deployment to Cloudflare Workers

The documentation is configured for deployment using **Wrangler**, the official Cloudflare Developer CLI.

### Prerequisites

1. **Node.js:** Ensure Node.js (version 18+) is installed on your local host.
2. **Wrangler CLI:** Install Wrangler globally or run it directly using `npx`:
   ```bash
   npm install -g wrangler
   ```
3. **Cloudflare Account:** An active Cloudflare account is required with ownership of the `rustshare.io` domain zone.

---

### Step-by-Step Deployment Guide

Follow these steps to deploy the static documentation site directly to the Cloudflare Edge network:

#### 1. Authenticate Wrangler
Log in to your Cloudflare developer account via your browser:
```bash
wrangler login
```

#### 2. Run and Validate Locally
Before deploying, you can run a local emulation of the Cloudflare Workers environment. This spins up a local worker that mounts and serves our static assets exactly as it would behave in production:
```bash
# Start Wrangler Dev Server
wrangler dev
```
Open `http://localhost:8787` in your browser to verify routing, theme toggling, clipboard copies, and search functionality.

#### 3. Deploy to Production
To publish the documentation site live to Cloudflare's global edge network under **docs.rustshare.io**, execute:
```bash
wrangler deploy
```
Wrangler will:
- Read `wrangler.toml` definitions.
- Index and compile the static assets in the current folder.
- Deploy the routing script `worker.js` and bind the `ASSETS` folder namespace.
- Apply the production DNS routing rules mapped in the `routes` block.

---

## 🌐 Custom Domain Routing

The custom domain routing is managed automatically via the `routes` block inside [wrangler.toml](wrangler.toml):

```toml
routes = [
  { pattern = "docs.rustshare.io/*", zone_name = "rustshare.io" }
]
```

To ensure the custom route connects successfully:
1. Ensure the domain `rustshare.io` is added to your Cloudflare Dashboard account as an active zone.
2. Go to **DNS Settings** in your Cloudflare dashboard for `rustshare.io`.
3. Add a placeholder DNS record for `docs` (e.g., a CNAME pointing to `@`, or a dummy A record pointing to `192.0.2.1` proxied through Cloudflare). This is required so Cloudflare knows to route the traffic of `docs.rustshare.io` through its edge network, triggering the Worker.
