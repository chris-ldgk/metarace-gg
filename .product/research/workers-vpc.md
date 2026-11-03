# S-6: Workers VPC (research, 2026-11-03)

**Verdict:** It is technically viable. cloudflared runs on the bare-metal host with outbound-only connections and no open inbound port. Workers call it through the `vpc_services` binding using `env.X.fetch()`. **Workers VPC is still in beta**: free during the beta, with no GA date, SLA, post-beta pricing or latency numbers published.

## Facts
- Setup: create a remotely-managed tunnel, then run `wrangler vpc service create <name> --type http --tunnel-id … --hostname …`, then add `"vpc_services": [{binding, service_id}]` to the wrangler config. cloudflared must be ≥ 2025.7.0 and needs outbound UDP 7844.
- One VPC *Service* = one host:port. Adding a host means a new binding and a **redeploy**. Bindings are checked at deploy time.
- The alternative is a VPC *Network* binding (public beta since Apr 2026), which reaches any IP behind a tunnel or mesh and picks the target per request at runtime. This fits multiple regions better (D-023).
- Workers VPC adds **no auth** at the origin. The orchestrator still needs app-level auth, e.g. HMAC-signed requests.
- Limits: 1000 services per account. Otherwise the standard Workers limits apply (6 concurrent outbound connections, 30 s CPU).
- Reverse path (orchestrator → cloud): call a Worker endpoint protected by an Access service token (`CF-Access-Client-Id/Secret` headers; max 50 tokens per account). The Worker then reaches the Durable Object through a stub, since DOs can't be reached from the Internet directly.
- Fallbacks if VPC disappoints: (a) plain Tunnel + an Access service token on per-host hostnames, with host selection driven by data in D1/KV and no redeploy; (b) the orchestrator pulls from a Queue over HTTP (GA).

## Still to verify hands-on
- p50/p95 latency from a deployed Worker
- Whether a loopback target works on the tunnel host
- Whether service-token requests carry `Cf-Access-Jwt-Assertion`

Sources: developers.cloudflare.com/workers-vpc/ (overview, get-started, vpc-services, vpc-networks, tunnel, limits, pricing, changelog), blog.cloudflare.com/workers-vpc-open-beta/, cloudflare-one service-tokens & account-limits, queues pull-consumers.
