# Domain purchase + Billing rollout: status and deployment requirements

Running doc, kept current while this work is in progress. Covers three repos: `domain_management` (`ais_domains`), `Billing`, and `portal`. The detailed step-by-step implementation plan lives outside this repo (a local Claude Code plan file); this doc is the durable, shared record of **what's actually landed, what's left, and what deploying any of it requires** — so it survives a session boundary and is readable by anyone on the team, not just whoever is driving the agent.

Last updated: 2026-09-28 (Billing's subscription/invoice/credit-ledger surface — `BillingAdminService` — landed; contract vendored in `ais_proto`, implemented in `Billing`, wired into `domain_management` and `portal`).

---

## Status at a glance

| Piece | Status |
|---|---|
| `ais_domains`: vhost backend variants + freeform vhost API | Done |
| `ais_domains`: challenge CNAME automation (`ensure_challenge_cname`) | Done, called from the `register` job |
| `ais_domains`: DNS record CRUD (`ListDnsRecords`/`Create`/`Update`/`Delete`) | Done |
| `ais_domains`: async job worker (claim/retry/backoff/reap/wait) | Done |
| `ais_domains`: Cloudflare Registrar client (search/check/register) | Done |
| `ais_domains`: Registrar purchase guardrails (pricing/caps) | Done (`src/purchasing/`) |
| `ais_domains`: `SearchDomains`/`QuoteDomain`/`CreateOrder`/`GetOrder`/`ListOrders` RPC bodies | Done |
| `ais_domains`: `register` job's real body (registrar purchase → zone → CNAME → issuance) | Done (`src/worker/register.rs`) |
| `Billing`: Stripe client + webhook verification + PaymentIntent gRPC | Done |
| `ais_domains`: `BillingClient` (calls Billing) | Done, called from `CreateOrder` and the `register` job |
| `ais_domains`: worker's "not ready yet" job outcome (payment still pending) | Done (`JobOutcome::Waiting`) |
| `portal`: Stripe webhook route → forwards to Billing | Done, but placeholder — see Deployment requirements below |
| `ais_domains`: BYO domain lifecycle (`AddDomain`/`GetDomain`/`ListDomains`/`RemoveDomain`/`VerifyDomainNow`/`DetachDomain`) | Done |
| `ais_domains`: `WatchDomain` (streaming status events) | Done (server-side polling, 5s interval) |
| `ais_domains`: Cloudflare account members (`InviteDomainMember`/`ListDomainMembers`/`RemoveDomainMember`) | Done (`src/cloudflare/members.rs`) |
| `ais_domains`: `ListCertificates`/`ForceRenew`/`PublishNow`/`ListReleases` | Done |
| `ais_domains`: every gRPC RPC this service declares | **All implemented** — no stub-tracking tests remain |
| `Billing`: `BillingAdminService` (plans/subscriptions/invoices/credit ledger, `AuthClient` for RBAC) | Done — 11 RPCs, vendored in `ais_proto`, wire-format pin-tested |
| `ais_domains`: `create_order` gated on `Billing::GetOrganizationBillingStatus` (fail-closed) | Done — see sequencing risk below, this is a new hard runtime dependency |
| `portal`: `/v1/billing/subscription*`, `/v1/billing/invoices` routes | Done |

**The domain purchase flow is now code-complete end to end** — `SearchDomains`/`QuoteDomain`/`CreateOrder` → Stripe payment via Billing → `register` job → Cloudflare registrar purchase → zone → CNAME → cert issuance → active domain. **As of Phase 7, so is the rest of the service**: BYO onboarding (two-step TXT ownership proof → DNS pointing checks → issuance), domain lifecycle reads/writes, streaming status watch, Cloudflare account member invites, and the ops surface (certificates, forced renewal, publishing). **None of it can be exercised for real yet**: it needs real Cloudflare registrar/zones/challenge/members tokens, a deployed Billing with real (or Stripe-test-mode) keys, and `Purchasing.enabled = true` — see Deployment requirements below. What has no automated test coverage: the `register` job's `Succeeded`-path walk through registration, and `VerifyDomainNow`'s live-DNS-and-issuance path (both need real external systems at once); everything else (guardrails, RPC authz, DB-driven dispatch logic, the new `db::domains` read/write surface, the Cloudflare members client's request/response shapes per Cloudflare's own docs) has real test coverage — 187 tests passing across the full suite.

---

## What's left to implement

### `ais_domains` (`/mnt/Developer/Artisan_Hosting/Platform/domain_management`)

- ~~Pricing/purchasing guardrails~~ — done, `src/purchasing/mod.rs`: markup + margin + cap math against `Config::Pricing`, TLD allowlist check, daily/monthly order caps against `Config::Purchasing`. 6 unit tests, no network.
- ~~Purchasing RPC bodies~~ — done, `src/grpc/service.rs`: `SearchDomains`, `QuoteDomain` (writes `domain_quotes`), `CreateOrder` (the money path: guardrails → `self.billing.create_payment_intent(...)` → `domain_orders` row → `client_secret`/`publishable_key` back to caller — needs a real access token + elevated token + `Purchasing.enabled = true` to get past the first checks), `GetOrder`/`ListOrders` (reads, `authz::scope`). New `src/db/orders.rs` for `domain_quotes`/`domain_orders`.
- ~~`register` job body~~ — done, `src/worker/register.rs`: `awaiting_payment` polls `BillingClient::get_payment_intent` (not-yet-succeeded → `JobOutcome::Waiting`, doesn't burn a retry attempt); once paid, `registrar::register`/`registration_status` → `zones::ensure()` → `dns::ensure_challenge_cname()` → issue+install cert → `vhost::snippet::ensure()` → mark domain active → mark order complete. Fully DB-state-driven so it survives being claimed/reclaimed across a minutes-long wait. This was the last piece of the actual purchase flow — the whole `CreateOrder` → paid → registered → live-domain path is now code-complete (not yet live-tested, see below).
- ~~Worker outcome extension~~ — done, landed as `JobOutcome::Waiting` in `src/worker/mod.rs`: requeues on a short fixed delay and decrements `attempts` back down (`GREATEST(attempts - 1, 0)`) so waiting on an external event never spends down the retry budget or triggers exponential backoff meant for real failures.
- ~~Remaining lifecycle/member/ops stubs~~ — done (Phase 7): `add_domain` (BYO, two-step TXT ownership proof; new `migrations/0006_domain_lifecycle.sql` adds `ownership_token`/`ownership_verified`), `get_domain`/`list_domains` (`src/db/domains.rs` grew a full `DomainRow` + `find_full`/`find_full_by_id_or_fqdn`/`list`), `remove_domain` (soft delete, optional Cloudflare zone deletion via new `cloudflare::zones::delete`), `verify_domain_now` (ownership TXT check → edge/CNAME checks → issuance), `watch_domain` (streaming via server-side polling, `tokio::sync::mpsc` + `ReceiverStream`, no new dependency needed), `detach_domain`, Cloudflare account members (new `src/cloudflare/members.rs`, verified against Cloudflare's current API docs — `roles` is an array of role-id strings resolved by name via `GET .../roles`, and the whole scheme is account-wide, not zone-scoped, which the module documents), `list_certificates`, `force_renew`, `publish_now`/`list_releases` (new `src/db/releases.rs`).
- **New shared function**: `acme::issue_and_install` (`src/acme/mod.rs`) — the CLI's `issue_one`, `ForceRenew`, `VerifyDomainNow`, and the `register` job's `finish_registration` all now call the same issuance core instead of four copies of "should this renew" drifting apart.
- ~~Stub-test cleanup~~ — done, folded into the same pass: `authorization_contracts::the_unimplemented_rpcs_say_so_rather_than_answering` is deleted; `every_stub_documents_its_authorization`'s threshold is now `assert_eq!(call_sites, 1, ...)` (the one survivor is `convert_vhost`'s own unrelated "structured template" branch).
- **Everything `ais_domains`'s proto declares is now implemented.** Nothing is stubbed. What's left is deployment (below), the two documented not-yet-tested live paths (`register`'s `Succeeded` walk, `VerifyDomainNow`'s live-DNS-and-issuance path), and whatever the platform asks for next.

### `Billing` (`/mnt/Developer/Artisan_Hosting/Platform/Billing`)

Functionally done for what `ais_domains` needs today, plus the subscription/invoice/credit-ledger surface (`BillingAdminService`) that `ais_domains` doesn't call yet but `portal` does. Known follow-ups, not blocking:
- ~~`test_calculate_costs_scaled` fails~~ — fixed as part of the aggregator time-weighting bug fix in `artisan_lib` (unpublished, see that repo's own notes); `calculate_costs` itself was replaced by `overage::calculate_pool_overage`.
- No CLI/ops tooling yet beyond `serve`/`migrate` — fine for now given current scope.
- Billing now has its own `AuthClient` (talks to `ais_auth` to evaluate RBAC policy and validate elevated tokens) — this is a new outbound dependency Billing didn't have before. See Deployment requirements below.
- GPU credit RPCs (`GetCreditBalance`/`TopUpCredit`/`DebitCredit`/`PreflightCreditCheck`) are implemented and contract-vendored but have no caller yet — `RunpodManager`'s integration is deliberately deferred pending a Runpod-side rework.

### `portal` (`/mnt/Developer/Artisan_Hosting/Platform/portal`)

- `system::billing`'s `create_payment_intent`/`get_payment_intent` client methods exist but nothing calls them yet (`#[allow(dead_code)]`) — will matter once there's a dashboard "pay for this domain" flow, not required for the CLI/API purchase path.
- No admin/dashboard UI for any of this yet (out of scope until asked for).

---

## Deployment requirements

Nothing above works end-to-end against real money until **all** of the following are true. None of them are done yet.

### Billing service
- [ ] Deployed somewhere reachable on the private network (a real host/port, not `127.0.0.1:50061`).
- [ ] mTLS certificate material issued for identity `ais_billing` (cert/key/CA, the same internal-CA process every other `ais_*` service uses), placed wherever `MTLS_CERT_PATH`/`MTLS_KEY_PATH`/`MTLS_CA_PATH` (or the defaults under `/etc/artisan/tls/`) expect it.
- [ ] Its own `DATABASE_URL` (a MySQL/MariaDB instance/schema — can be a new schema on an existing server, doesn't need a dedicated instance).
- [ ] Real `STRIPE_SECRET_KEY`/`STRIPE_WEBHOOK_SECRET`/`STRIPE_PUBLISHABLE_KEY` (test-mode keys are fine for the first end-to-end run).
- [ ] `Billing migrate` run against it — now applies `0001_init.sql` through `0007_credit_ledger_idempotency.sql` (plans/subscriptions/invoices/credit ledger, not just the original PaymentIntent table).
- [ ] `Config.auth.grpc_addr` pointed at `ais_auth`'s real address (defaults to plaintext `127.0.0.1:50051`, same "only works same-host, no-TLS" caveat as every other service's default). Billing's new `AuthClient` calls `ais_auth`'s `evaluate_access`/token-validation RPCs to gate `CreateOrUpgradeSubscription`/`TopUpCredit` — without this pointed correctly, every subscription/credit purchase call fails.
- [ ] `ais_billing` reuses one mTLS identity for both directions (server to `ais_domains`/`portal`, client to `ais_auth`) — same dual-role pattern `ais_secretserver`/`RunpodManager` already use. Confirm `ais_auth` trusts `ais_billing` as a caller.
- [ ] Systemd unit / process supervision decided (no `.service` file exists yet, unlike `ais_domains.service`).

### `ais_auth`
- [ ] Migration `0007_rbac_subscription_policy.sql` applied — seeds the GLOBAL policy granting `purchase` on the `subscription` resource type. Without it, `evaluate_access` denies every `CreateOrUpgradeSubscription`/`TopUpCredit` call regardless of caller role.

### `ais_domains`
- [ ] `Config.billing.grpc_addr` pointed at Billing's real address (currently defaults to plaintext `127.0.0.1:50061`, which only works if Billing happens to run on the same host without TLS — fine for local dev, not for a real deployment).
- [ ] Real `CF_REGISTRAR_TOKEN`/`CF_ZONES_TOKEN`/`CF_CHALLENGE_TOKEN` — load-bearing since Phase 5.3: `SearchDomains`/`QuoteDomain`/`CreateOrder` call the real Cloudflare registrar `domain-search`/`domain-check` endpoints, not just in tests.
- [ ] Real `CF_MEMBERS_TOKEN` — load-bearing as of Phase 7: `InviteDomainMember`/`ListDomainMembers`/`RemoveDomainMember` call the real Cloudflare account-members API. Needs **Super Administrator** on the Cloudflare account per Cloudflare's own docs for managing members at all.
- [ ] `Purchasing.enabled` flipped to `true` in `ais_domains.json` (defaults `false` — the kill switch) once ready to actually spend money.
- [ ] **Sequencing risk, new as of this update**: `create_order` now calls `Billing::GetOrganizationBillingStatus` and fails closed (refuses the order) if Billing can't be reached — this is deliberate, not a bug, but it means domain purchasing goes from "works" to "refuses everything" the moment `ais_domains` ships this gate, unless Billing is already up and reachable at `Config.billing.grpc_addr`. Don't roll this build out ahead of Billing's own deployment.
- [ ] `Pricing.tld_allowlist`/`max_price_cents`/etc. reviewed for real values before enabling.
- [ ] `Dns.edge_ipv4`/`edge_ipv6` set to the platform's real edge addresses — `AddDomain`'s required-records response and `VerifyDomainNow`'s pointing check are both no-ops (or silently permissive) while these are empty, which is the safe default for local dev but not for a real BYO onboarding flow.

### `portal`
- [ ] `BILLING_GRPC_ADDR` set once Billing is deployed (no fallback default exists on purpose — see `system::billing`'s module doc; until this is set, `POST /v1/webhooks/stripe` correctly fails with `Status::unavailable`).
- [ ] `POST /v1/webhooks/stripe`'s public URL registered with Stripe's dashboard as the webhook endpoint.
- [ ] Confirm Portal's own mTLS identity (`ais_portal`) is authorized to reach Billing (same internal-CA trust as every other cross-service call here).

### Verification checklist, once the above are true
- [ ] `cargo test` clean in `domain_management` and `Billing` (already true as of this doc's last update).
- [ ] A real Stripe test-mode `CreateOrder` → webhook → `register` job → active domain, on a cheap allowlisted TLD, with no manual DNS step.
- [ ] `dig _acme-challenge.<domain> CNAME` resolves automatically post-purchase.
- [ ] Stripe CLI (`stripe trigger payment_intent.succeeded`) reaches Billing through Portal's real webhook route, not just through a direct `grpcurl` call to Billing.
