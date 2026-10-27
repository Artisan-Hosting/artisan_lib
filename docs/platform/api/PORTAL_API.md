# Portal API — `/v1` reference

Portal is the platform's **single public entry point**. Everything a browser,
script or operator touches goes through it; the services behind it
(`ais_auth`, `ais_secretserver`, `ais_domains`, `RunpodManager`) bind private
addresses and speak mutual TLS to each other, never to the outside.

**Status:** written 2026-09-20 against the working tree after RBAC Phases 1–4,
6 and 7. Read from the route table and handlers, **not** exercised against a
running Portal — no HTTP-level test harness exists yet. Treat request/response
shapes as accurate-as-written and verify the first time you call one.

---

## 1. Conventions

**Prefix.** Every route below is under `/v1`. There is no `/v0` (it exists in
the source, commented out).

**Methods.** `GET` and `POST` only, plus `OPTIONS` for preflight. **Mutations
that would naturally be `PUT`/`PATCH`/`DELETE` are `POST`** — the CORS layer
allows only those three methods, so widening the verb set means widening CORS.
That is why you will see `POST /v1/secrets/delete` rather than `DELETE
/v1/secrets`.

**CORS.** `allow_any_origin()`, headers `content-type` and `authorization`.
⚠️ Any origin can call this API with a token it already has. That is a
deliberate current state, not a reviewed decision — see §6.

**Authentication.** A bearer token on every route except `POST /v1/login`,
`POST /v1/auth/accept-invite`, the public signup routes in §3.1 and `GET /v1/discover`:

```
Authorization: Bearer <access_token>
```

**Elevation.** A few writes need a *second*, short-lived token in the request
body, minted by `POST /v1/admin/elevate` after a fresh password check. A normal
access token is **not** accepted in its place — the services check the token's
type, not just its validity. Anything that spends money, moves a tenant's
resources between organizations, or changes who someone is, needs one.

**Response envelope.** Most responses — success or failure — are the same
shape, and **HTTP status is almost always 200**. Check the body, not the status
line:

```json
{
  "status": "success",
  "data":   { },
  "errors": []
}
```

```json
{
  "status": "error",
  "data":   null,
  "errors": [
    { "code": "NotAuthorized", "message": "…", "details": [] }
  ]
}
```

`code` is one of: `NodeNotFound`, `ProjectNotFound`, `InvalidCredentials`,
`NotAuthorized`, `InternalError`, `TimedOut`, `Whoops`.

⚠️ **Three routes do not use the envelope.** `POST /v1/login` and
`POST /v1/auth/accept-invite` return `{"auth": "...", "refresh": "..."}` —
note `auth`, not `access_token`. `POST /v1/whoami` returns `{"you": {...}}`.

⚠️ **Authentication failures are `500`, not `401`/`403`.** A missing, expired
or insufficient token is raised as a warp rejection, and Portal installs no
recovery handler, so it reaches the client as `500` with an **empty body**.
Treat any non-200 as "not authenticated or not allowed" and do not try to parse
it. This is a bug (§6), not a convention.

⚠️ **The refresh token cannot be used.** `/v1/login` returns one, but no route
accepts it — `auth_refresh` exists in Portal's gRPC client and is unrouted. A
session ends when the access token expires.

For routes that front another service, the upstream gRPC status maps to a code:
`Unauthenticated` → `InvalidCredentials`, `PermissionDenied` → `NotAuthorized`,
`InvalidArgument` → `Whoops`, anything else → `InternalError`. An
`InternalError` reading "…unavailable" generally means the backing service is
down, not that your request was wrong.

---

## 2. Roles and who may call what

Five roles: `Super`, `Admin`, `Controller`, `Viewer`, `Audit` (plus `None`).

**The one thing to internalise: `Admin` is an admin *of one organization*.**
`Super` is the only platform-wide role. Before RBAC Phase 7, Portal treated
`Admin` as fleet-wide in several handlers; that was a cross-tenant bug and is
fixed. If you are writing a client, do not assume an Admin can see anything
outside their own org.

Two different gates apply, and the distinction matters when you read the table
in §3:

| Gate | Meaning |
|---|---|
| **Route role** | A coarse filter on the route itself. `Super` passes every route-role gate; otherwise the caller's role must match exactly. |
| **Per-resource** | The real decision, made per project/secret/domain by `ais_auth` (or by `ais_domains` for domains). The route says `auth`, and the handler asks. |

A route marked `auth` is *not* unauthenticated — it means any valid token
reaches the handler, which then decides per resource. Those are the routes
where a `Viewer` and an `Admin` see genuinely different results.

Actions used by the per-resource checks: `Read` (see it), `Write` (change its
configuration), `Control` (start/stop), `Delete`, `Grant`, `Purchase`.

---

## 3. Endpoints

### 3.1 Authentication

| Method | Path | Gate | Notes |
|---|---|---|---|
| POST | `/v1/login` | — | `{ "email", "password" }` → token pair |
| POST | `/v1/whoami` | auth | The caller's own claims |
| POST | `/v1/auth/accept-invite` | — | `{ "token", "display_name", "password" }`; the invite token authenticates it |
| POST | `/v1/auth/signup` | — | **Public self-signup, step 1.** `{ "email", "display_name", "password" }` → `{}`. Always the same answer for a well-formed request, whether or not the address already has an account (the *email* differs). Mail is throttled per address (1/min, 5/day). **No captcha here**: Cap is checked by the dashboard backend, and these three paths must be allow-listed to it at the reverse proxy ([SIGNUP_GATING](../operations/SIGNUP_GATING.md)) |
| POST | `/v1/auth/signup/resend` | — | `{ "email" }` → `{}`. Same always-OK answer; only does anything if a signup is pending |
| POST | `/v1/auth/signup/verify` | — | `{ "token" }` from the emailed link. Creates the user **and a draft organization they are Admin of**, returns `{ "auth", "refresh" }` like login. A used, expired or made-up link gets one generic error |
| POST | `/v1/account/organization` | auth | `{ "name" }`. The new Admin names their draft organization, **once**; returns `{ id, name, slug }`. The slug is derived from the name and numbered on collision |

Signup errors: 400 for validation (the message is user-facing). Without the proxy allow-list these routes are open to the internet, protected only by the per-address mail throttle. There is no per-IP limit in Portal; put one at the edge.

### 3.2 Admin — organizations, users, invites

All require route role **Admin** (so Admin or Super) **and** the raw bearer
token. The three marked 🔐 also require `elevated_token` in the body.

| Method | Path | Notes |
|---|---|---|
| POST | `/v1/admin/elevate` | 🔑 mints the elevated token; body `{ "password" }` |
| POST | `/v1/admin/organizations` | 🔐 create an organization. **Super only** in `ais_auth` |
| GET | `/v1/admin/organizations` | Super sees all; Admin sees their own |
| GET | `/v1/admin/organizations/{org_id}/users` | Admin restricted to their own org |
| POST | `/v1/admin/users/{user_id}/org` | 🔐 reassign a user's org/role. An Admin can never grant `Super` |
| POST | `/v1/admin/invites` | 🔐 invite a user |
| GET | `/v1/admin/invites` | |
| POST | `/v1/admin/invites/{invite_id}/revoke` | 🔐 |
| GET | `/v1/admin/organizations/{org_id}/runners` | |
| POST | `/v1/admin/runners/{runner_id}/org` | 🔐 attach a runner to an org |
| POST | `/v1/admin/runners/{runner_id}/org/remove` | 🔐 |

> There is still **no "remove a user from an org"** RPC — only reassignment.
> It is a known gap (RBAC Phase 7 §2), and it needs a product decision first:
> detach the user leaving them orgless, or delete the account.

### 3.3 Repos

| Method | Path | Gate | Notes |
|---|---|---|---|
| GET | `/v1/repos` | role **Admin** | Super sees the fleet; an Admin sees only repos whose org matches theirs |
| POST | `/v1/repos/deploy` | role **Admin** + token | Places a repo on nodes and can seed its secrets |

⚠️ **Phase 7 tightened deploy.** A new repo now needs a **Super**; a non-Super
Admin may only redeploy a repo their organization already owns (`Write` on it).
Placing new software on shared nodes is a platform operation.

#### Customer deploy (self-serve)

The path for an organization's own Admin. No node choice, no operator.

| Method | Path | Gate | Notes |
|---|---|---|---|
| POST | `/v1/repos/inspect` | any signed-in user | Body `{url, token?}`. GitHub only: returns `{supported, owner, repo, private, default_branch, branches, stack, suggestion{install,build,run}, note}`. Another host returns `supported:false` and the caller fills the commands in by hand. Suggestions are editable starting points. A server-side `GITHUB_API_TOKEN` raises GitHub's 60/hour unauthenticated limit |
| POST | `/v1/projects` | role **Admin** + token | Body `{repo{user,repo,branch,server?,token?}, commands{build_command?,install_command?,run_command}, secrets[{key,value}], environment_id?}`. Needs an active plan (fails closed). Portal picks the node (healthy, fewest apps) and registers the project to the caller's organization. Returns `{id, node_id, state, started, checkout}` |
| GET | `/v1/projects/{id}/deployment` | `Read` on the project | `{id, node_id, state, message, port, build, log_tail}`; `state` is `preparing`, `building`, `built`, `starting`, `running` or `failed`. Read-only: a `built` app is started with the normal control route |

#### Addresses (go live)

Three ways to give a deployed app an address, all ending in a vhost on the edge that points at the app's
node and port. The caller never types a server or a port: Portal reads both from the node. Portal decides
the **project** (`can_access_project`); `ais_domains` decides the **domain** from the caller's own token.

| Method | Path | Gate | Notes |
|---|---|---|---|
| POST | `/v1/projects/{id}/address` | role **Admin** + `Write` on the project | Body `{mode:"free", name}` (takes `<name>.<free zone>`) or `{mode:"custom", fqdn}` (a domain the organization holds, a subdomain of one, or one the caller brings). 400-style `Whoops` if the app has no port yet. Returns `{fqdn, status, has_vhost, outcome, notes[], required_records[{type,name,content,note}]}`; `required_records` is the DNS the customer still has to add when we do not hold the zone |
| GET | `/v1/projects/{id}/address` | `Read` on the project | `{app_running, addresses[{fqdn, status, state, message}]}`. `state` is `reserving`, `dns`, `certificate`, `live`, `app_down`, `not_attached`, `failed` or `checking`. **`app_running` is the node's own report, not a request to the public name**: fetching a customer-controlled name from Portal would let their DNS aim Portal at anything |
| POST | `/v1/projects/{id}/address/check` | `Read` on the project | Body `{fqdn}`; the name must belong to this project. Asks `ais_domains` to re-check DNS and the certificate now. Returns one `{fqdn,status,state,message}` |
| GET | `/v1/address/free?name=` | any signed-in user | `{available, fqdn, reason, free_zone}`. Checks shape, the operator's reserved names, then whether it is taken. Send an empty `name` to learn the zone |
| GET / POST | `/v1/domains/reserved` , `/v1/domains/reserved/remove` | **Super** (enforced by `ais_domains`) | Reserved-name rules for the free zone. A rule is `{kind: exact\|prefix\|range, prefix, digits, min_value, max_value, note}`; `c00000000`-`c99999999` is `range`, prefix `c`, 8 digits, 0 to 99999999. POST body is a rule, remove body is `{id}`. No restart needed |

Buying a domain *for* an app: `POST /v1/domains/orders` now resolves the named `runner_id` to its node and
port and sends them with the order, and the domain controller attaches the vhost the moment the domain
finishes. If the app is not running yet the order is placed without a backend and the domain lands
unattached, as before. `CreateOrder` now also requires the caller may write the runner it names.

Order of refusal: validation, billing gate, claiming the id for the caller's
organization, node choice, secret seeding. Nothing touches a node before all of
those pass, and a retry after a late failure reuses the id the caller already owns.

### 3.4 Secrets

Route gate is `auth` on all four; `ais_secretserver` authorizes each call
against the caller's own token, on resource `Secret` `{runner_id}/{environment_id}`.

| Method | Path | Action | Body / query |
|---|---|---|---|
| GET | `/v1/secrets` | Read | `?runner_id=&environment_id=` |
| POST | `/v1/secrets` | Write | `{ "runner_id", "environment_id", "secret_key", "value" }` |
| POST | `/v1/secrets/update` | Write | `{ …, "new_value" }` |
| POST | `/v1/secrets/delete` | Delete | `{ "runner_id", "environment_id", "secret_key" }` |

⚠️ **Secret *values* are returned in plaintext to anyone with `Read`**, and by
the seeded policy that includes `Viewer`, `Controller` and `Audit` of the owning
organization. If that is not what you want, set an org policy denying
`secret`/`read` for those roles. This is Phase 1 seed behaviour, not new.

### 3.5 Domains — **new in Phase 6**

Route gate is `auth` on all six. Portal makes **no** authorization decision
here: it cannot resolve a domain to an organization (that row lives in
`ais_domains`' database), so it forwards the caller's token and `ais_domains`
decides against the row. See [the domain controller's
section](../operations/RBAC_OPERATORS_RUNBOOK.md) §2.5 for the model.

Only the RPCs the dashboard calls are exposed; the rest of `ais_domains`' surface has no Portal route.
Mutations are `POST` (this layer's CORS allows only GET/POST/OPTIONS), so deletes are `.../delete` and `.../remove`.

| Method | Path | Who | Notes |
|---|---|---|---|
| GET | `/v1/domains` | org-scoped | `?organization_id=&unassigned_only=&limit=&offset=` |
| GET | `/v1/domains/findings` | **Super** | Scanner findings; they name host file paths, so there is no org-scoped version |
| GET | `/v1/domains/vhosts` | org-scoped | Adopted hand-written vhosts, with drift flags |
| POST | `/v1/domains/assign` | owner/Super | Set or clear a domain's org and runner |
| POST | `/v1/domains/attach` | owner + runner `Write` | Point a domain at a runner |
| POST | `/v1/domains/rescan` | **Super** | Re-read the nginx tree; slow with `check_dns` |
| POST | `/v1/domains` | any member (org stamped by `ais_domains`) | Body `{fqdn, runner_id?, backends?[{node_id,port}], organization_id? (Super only)}`. Returns `{domain, outcome(linked\|adopted\|provisioned\|needs_dns), notes[], required_records[{type,name,content,note}]}`; with `needs_dns` the customer creates those records at their DNS host, then calls verify |
| GET | `/v1/domains/{id}` | owner/Super | `{id}` is the id or the fqdn. The listing fields plus `parent_fqdn, auto_renew, challenge_target, last_error, certificates[], records[{purpose,type,name,content,drifted}]`. Shares its path shape with the literal routes below, which win |
| POST | `/v1/domains/{id}/verify` | owner/Super | Re-check DNS and the certificate now. Returns the domain as `GET` does |
| POST | `/v1/domains/{id}/detach` | owner/Super | Takes the vhost out of service; the domain, DNS and certificates stay |
| GET | `/v1/domains/certificates` | org-scoped | `?organization_id=&expiring_within_days=`. Array of `{domain_id,key_type(ecc\|rsa),serial,not_before,not_after,renew_after,fail_count,last_error}` |
| POST | `/v1/domains/{id}/force-renew` | owner/Super, **elevated** | Body `{key_type?, elevated_token}`. Issues both key types regardless of `key_type`; rate-limited (one per hour per domain). Returns `{job_id}` |
| GET / POST | `/v1/domains/{id}/dns-records` | owner/Super | List, or create `{type,name,content,ttl?,proxied?}`. `ttl` 1 or absent means automatic |
| POST | `/v1/domains/{id}/dns-records/{record}` | owner/Super | Update; same body as create |
| POST | `/v1/domains/{id}/dns-records/{record}/delete` | owner/Super, **elevated** | Body `{elevated_token}` |
| GET / POST | `/v1/domains/{id}/members` | owner/Super | List, or invite `{email, role?}` (role defaults to "Domain DNS"). Members are `{domain_id,email,cf_member_id,role,status(pending\|accepted\|removed),invited_at}` |
| POST | `/v1/domains/{id}/members/remove` | owner/Super | Body `{email}` |
| POST | `/v1/domains/{id}/validate-vhost` | owner/Super | Body `{server_block}`. `nginx -t` against a staged tree; writes nothing. Returns `{nginx_ok,nginx_output,new_findings[],corrected}` |
| POST | `/v1/domains/{id}/apply-vhost` | owner + runner `Write`, **elevated** | Body `{server_block, elevated_token, dry_run?}`. Returns `{applied,diff,validation}` |
| GET | `/v1/domains/vhost-content` | org-scoped | `?path=` of an entry from `/v1/domains/vhosts`. Returns `{path,content,file_sha256}`. The path is a lookup key; anything not in the caller's adopted-vhost list is 404 |

"org-scoped" means: a `Super` may pass any `organization_id` (or none, for the
fleet); **everyone else is pinned to their own organization whatever they
send.**

**`GET /v1/domains`** →

```json
{ "status": "success",
  "data": { "entries": [ {
      "id": "12", "fqdn": "example.com",
      "organization_id": "", "runner_id": "ab12cd34",
      "source": "imported", "status": "active",
      "serves_tls": true,
      "vhost_paths": ["sites-enabled/example"],
      "cert_dirs": ["/etc/nginx/certs/_.example.com"],
      "expires_at": 1790000000,
      "findings": ["cert_expiring"]
  } ], "total": 41, "unassigned": 12 },
  "errors": [] }
```

`source` is `purchased` | `byo` | `imported` | `unspecified`. `status` is
`pending_payment` | `registering` | `provisioning_dns` | `pending_dns` |
`issuing` | `active` | `renewing` | `error` | `removed` | `unspecified`. An
unrecognised value arrives as `unknown(<n>)` rather than failing the listing —
`ais_domains` may learn a value before Portal is rebuilt.

An **empty `organization_id` is normal and durable**, not an error: it means
nobody has claimed the domain yet.

**`POST /v1/domains/assign`**

```json
{ "id_or_fqdn": "example.com",
  "organization_id": "…", "runner_id": "ab12cd34",
  "clear_org": false, "clear_runner": false,
  "elevated_token": "" }
```

An empty string means *leave alone*; clearing is explicit via `clear_*`, so
"not specified" and "remove it" can never be confused. `elevated_token` is
required only to move a domain that already belongs to an organization.

⚠️ **Claiming an unassigned domain is Super-only.** An org Admin cannot take an
unowned name, because the first Admin to ask would otherwise own it. Bulk
assignment is the operator's CLI (`ais_domains apply`).

**`POST /v1/domains/attach`**

```json
{ "id_or_fqdn": "example.com",
  "runner_id": "ab12cd34",
  "backends": [ { "node_id": "2973453917896704", "port": 8093 } ],
  "extra_names": ["www.example.com"],
  "no_http_redirect": false }
```

Needs `Write` on the runner **and** ownership of the domain. More than one
backend becomes a balanced upstream; re-attaching with a different set is how
scaling out reaches the edge. `ais_domains` stages the tree and runs `nginx -t`,
rolling the file back itself if the tree is rejected, and **never replaces a
hand-written vhost**.

### 3.6 Nodes — ⚠️ **Super-only since Phase 7**

| Method | Path | Gate |
|---|---|---|
| GET | `/v1/nodes` | **Super** |
| GET | `/v1/node/{node_id}` | **Super** |
| GET | `/v1/node/{node_id}/git-config` | **Super** |
| POST | `/v1/node/{node_id}/git-config` | **Super** |
| GET | `/v1/node_reload/{node_id}` | **Super** |
| GET | `/v1/node/{node_id}/watchdog/config` | auth → `Write` on the project |
| POST | `/v1/node/{node_id}/watchdog/config` | auth → `Write` on the project |

**This is a breaking change for any client that showed a Nodes page to an
`Admin`.** A node is shared infrastructure hosting every organization's
runners, and nothing ties one to a single org — its state lists every tenant's
projects and its git config decides what runs on it for all of them. If your
dashboard's node views went blank for a non-Super login, this is why.

### 3.7 Runners, usage, logs, control

All `auth`; the handler decides per project.

| Method | Path | Per-resource |
|---|---|---|
| GET | `/v1/runners` | `Read` on each; the list is filtered, not refused |
| GET | `/v1/runner/{runner_id}` | `Read` |
| GET | `/v1/runner/{runner_id}/git-info` | `Read` — repo/branch label only |
| GET | `/v1/runner/{runner_id}/config` | **`Write`** |
| POST | `/v1/runner/{runner_id}/config` | **`Write`** |
| GET | `/v1/usage/single/{instance_id}` | `Read` |
| GET | `/v1/usage/group/{runner_id}` | `Read` |
| GET | `/v1/logs/{runner_id}/{node_id}` | `Read` |
| GET | `/v1/control/{runner_or_instance}/{command}` | `Control` |

Notes worth knowing:

* **Reading a runner's config requires `Write`, not `Read`.** Config files can
  carry credentials, so viewing and editing sit at the same bar. In the seeded
  policy that means **Admin** — a `Controller` can start and stop a runner but
  can no longer open its config.
* **System apps** (`manager`, `gitmon`, `mailler`, `welcome`) are hidden from
  the runner list for everyone but a `Super`, and have no git info.
* `GET /v1/runners` **omits** what you cannot see rather than erroring. If an
  org lookup fails, the response says so rather than silently reporting an
  empty fleet.
* Control commands are `GET` — including the ones that stop a runner. Do not
  let a link prefetcher loose on them.

### 3.8 Discover

| Method | Path | Gate |
|---|---|---|
| GET | `/v1/discover` | ⚠️ **none** |

Unauthenticated. Confirm what it exposes before Portal faces the internet.

### 3.9 Billing and credit

Route gate is `auth` on everything except the Stripe webhook. Portal makes **no**
authorization decision here: it forwards the caller's own token to Billing, and
Billing asks `ais_auth` (`EvaluateAccess` on the `subscription` resource, whose
id is the **organization id**). Billing is not deployed yet, so until it is and
`BILLING_GRPC_ADDR` is set, every route below fails `unavailable` rather than
pretending.

Who may do what, per organization:

| Action | Who |
|---|---|
| Read plans, subscriptions, invoices, credit balance and ledger | `Admin`; other roles **only if the organization turned on "Let viewers see billing"** (default off) |
| Buy, upgrade, top up credit, pay an open invoice | `Admin`, **plus an elevated token** |
| Schedule a downgrade, cancel | `Admin` (`Write`); no elevated token — nothing is charged |
| `GET /v1/billing/plans` | any signed-in caller — it is the public Price Book |

`Super` passes everything, as elsewhere. The viewer switch is an ordinary org
policy row (`subscription`, `read`, `viewer`) written through
`POST /v1/admin/organizations/{id}/policy`, so it needs an elevated token too.

| Method | Path | Notes |
|---|---|---|
| GET | `/v1/billing/plans` | `?storefront=` (empty = all). Plan code, price in cents, allowances and overage rates |
| GET | `/v1/billing/subscriptions` | `?organization_id=`. One row per storefront |
| GET | `/v1/billing/subscription` | `?organization_id=&storefront=`. `404`-style `not_found` when never subscribed |
| POST | `/v1/billing/subscription/upgrade` | New subscription, or upgrade (prorated, charged now). Returns a Stripe `client_secret` to pay |
| POST | `/v1/billing/subscription/downgrade` | Takes effect at period end |
| POST | `/v1/billing/subscription/cancel` | Takes effect at period end; status becomes `canceled` then |
| GET | `/v1/billing/invoices` | `?organization_id=&storefront=&limit=&offset=`. Returns `{invoices, total}` |
| POST | `/v1/billing/invoices/retry` | Pay an `open` invoice again instead of creating a second one |
| GET | `/v1/billing/credits` | Balance in cents (may be negative) |
| GET | `/v1/billing/credits/ledger` | `?organization_id=&limit=&offset=`, newest first, `{entries, total}` |
| POST | `/v1/billing/credits/topup` | Minimum 2500 cents. Returns a Stripe `client_secret`; the credit lands only when Stripe's webhook reaches Billing |
| POST | `/v1/webhooks/stripe` | Stripe only. No bearer token; Billing verifies the HMAC over the raw body |

**Paying.** Anything that charges returns `stripe_client_secret` and
`stripe_publishable_key`. The browser confirms the card with Stripe.js — card
data never reaches us. Money is real only when Stripe's webhook arrives, so
after confirming, poll `GET /v1/billing/subscriptions` (or `/credits`) until it
changes; it usually takes a few seconds. An empty `stripe_client_secret` means
nothing is owed (a free plan).

**Subscription `status`:** `active`, `past_due` (the current period's invoice is
unpaid), `grace_period`, `suspended`, `deleted`, `canceled`. A renewal moves an
`active` subscription to `past_due` until its new invoice is paid; one that is
already unpaid is **not** renewed again. `canceled` is terminal for that period:
buying the plan again restarts the same row and never blocks the organization's
other purchases.

**Periods.** Billing's rollover job (runs only when purchasing is enabled)
applies a queued downgrade, ends a cancellation, advances one 30-day period and
issues the next invoice, once per period. There is no saved card, so the
customer pays each renewal in the dashboard; nothing is charged automatically.

---

## 4. Calling it

```bash
# note: .auth -- login does not use the envelope, and the field is not
# called access_token
TOKEN=$(curl -s -X POST https://api.artisanhosting.net/v1/login \
  -H 'content-type: application/json' \
  -d '{"email":"you@example.com","password":"…"}' | jq -r .auth)

curl -s https://api.artisanhosting.net/v1/domains \
  -H "authorization: Bearer $TOKEN" | jq .

# a write that costs a fresh password
ELEVATED=$(curl -s -X POST https://api.artisanhosting.net/v1/admin/elevate \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"password":"…"}' | jq -r .data.elevated_token)   # this one IS enveloped
```

Remember to check `.errors`, not the HTTP status.

---

## 5. What changed recently

| Change | Effect on clients |
|---|---|
| **Phase 7** — `Admin` is no longer fleet-wide | An org Admin loses access to other orgs' projects, config and secrets. Intended. |
| **Phase 7** — node routes are Super-only | A Nodes page shown to an Admin goes empty. **Breaking.** |
| **Phase 7** — `Super`'s role is no longer rewritten to the route's role | A Super's token now reaches backing services as `Super`, so cross-org calls work where they used to be denied. |
| **Phase 7** — config read/write needs `Write` | `Controller` and `Viewer` lose the config editor. |
| **Phase 7** — new repo deploys need `Super` | An Admin can redeploy their own org's repos only. |
| **Customer deploy** — `POST /v1/repos/inspect`, `POST /v1/projects`, `GET /v1/projects/{id}/deployment` added | New surface; `/v1/repos/deploy` is unchanged and stays the operator path. |
| **Go live** — `/v1/projects/{id}/address[/check]`, `/v1/address/free`, `/v1/domains/reserved[/remove]` added; `POST /v1/domains/orders` attaches the domain when it finishes if it names a running app | New surface; nothing removed. `CreateOrder` now refuses a `runner_id` the caller cannot write |
| **Billing** — `/v1/billing/*` documented; `GET /v1/billing/invoices` now returns `{invoices, total}` (was a bare array) | New surface; the invoice list shape is a change for anything that read the old one. |
| **Billing** — `/v1/billing/{plans,subscriptions,invoices/retry,credits,credits/ledger,credits/topup}` added | New surface; nothing removed. |
| **Phase 6** — `/v1/domains/*` added | New surface; nothing removed. |

Nothing was renamed or removed, so an existing client keeps working — it may
simply see less, depending on who is logged in.

---

## 6. Known gaps in this surface

Honest list, so nobody discovers these the hard way:

1. **CORS allows any origin.** With `authorization` as an allowed header, any
   site a logged-in user visits can call this API with a token it can reach.
   Pin the origins to the dashboard before this is public.
2. **`GET /v1/discover` is unauthenticated.** Audit what it returns.
3. **Control commands are `GET`.** Side-effecting reads are prefetchable.
4. **Secret values are readable by `Viewer`/`Controller`/`Audit`** of the
   owning org under the seeded policy (§3.4).
5. **Auth failures return `500` with an empty body** instead of `401`/`403`,
   because no `.recover()` handler is installed. Any client trying to
   distinguish "log in again" from "the server broke" cannot.
6. **Refresh tokens are minted but unusable** — no route accepts one.
7. **No HTTP-level tests exist.** Portal's handlers are verified by compilation
   and by unit tests on the decision helpers, never by a request. This document
   is written from the source.
8. **No pagination anywhere except `/v1/domains`.** Runner and repo listings
   return everything.
9. **No rate limiting and no request-id/correlation header.**
10. **`whoami` spells it `org_id`** while the rest of the platform uses
    `organization_id` (Phase 8 territory).
11. **Project ids collide across organizations.** The id is a hash of
    branch + repo + user, so two organizations deploying the same repository and
    branch get the same id; `POST /v1/projects` refuses the second ("already
    deployed"). Fix (v8.0.0): include the organization in the id, in the shared
    `artisan_middleware` derivation and every node-side path built from it.
12. **Self-serve deploys are verified by unit, route and throwaway-database
    tests only.** The node-side chain (clone, build, start, `PORT`, secrets) has
    not been run end to end on a real node.
13. **Deferred to v8.0.0:** build-tool and runtime version detection (Node,
    Python, ...), and plan-derived default CPU/RAM limits with an overage
    acknowledgement when they are changed. v1 enforces only "has an active plan".
14. **Go live is verified by unit, route-shape and throwaway-database tests only.**
    Cloudflare writes, wildcard certificate issue, the edge reload and the
    purchase worker's attach step have not run against real services. The
    worker's `go_live` step has no test of its own.
15. **A free name needs operator set-up first:** apply migrations 0008 and 0009,
    set `free_zone.zone` (e.g. `arhst.net`), record that zone as a managed domain
    with a wildcard certificate, set `dns.edge_ipv4/ipv6`, and add the
    `c00000000`-`c99999999` reservation (the Reserved names screen has a button).
    Until then `ClaimFreeAddress` answers "not turned on" or "not set up".
16. **"Live" does not probe the public name** (see the address table). It means the
    domain is active, has a vhost, and the node says the app is running.

---

## 7. Machine-readable spec

[`openapi.yaml`](openapi.yaml) in this folder is the same surface as an OpenAPI
3.0.3 document — 49 paths, 55 operations — and is what to generate clients
from. It supersedes `dashboard/api.yml`, which documents `/auth/login`,
`/auth/refresh` and `/vms/*`: none of them routes Portal serves today.

Surfaces that are planned but not built (the session manager's REST re-export,
`ais_vm`) are **deliberately absent** from it, so a generated client cannot
call something that will 404. They live in
[`PLANNED_SURFACES.md`](PLANNED_SURFACES.md) until they land.

---

## 8. Where the decisions live

* `portal/src/api/routes.rs` — the route table; the gates are visible here.
* `portal/src/api/helpers.rs` — `can_access_project`, `is_platform_operator`,
  the route-role gate. A test fails the build if a handler decides access from
  a role or from org membership instead of asking `ais_auth`.
* `portal/src/api/handler/` — one module per surface.
* `docs/operations/RBAC_OPERATORS_RUNBOOK.md` — the operator's view: §2.4
  (Portal scoping), §2.5 (the domain controller), §6 (configuration).
