# Platform reference docs (snapshot)

Read-only copies of the `Platform/docs/` files that the [8.0 plan set](../plans-8.0/README.md) cites, so the plans can link to
something that lives in this repository. `Platform/docs/` is not a git repository; these copies are, so they also give the specs
a history.

Copied 2026-10-27. Paths below are relative to `Platform/docs/` (the layout is mirrored here).

**Editing rule (decision D21, vendored):** the 8.0 plans change three of these files (taxonomy §4.1/§5/§8.3/§10, `PLATFORM_OVERVIEW.md` URN
examples, see W1.5, W2.4, W2.10). Make those edits in the copy here, then copy the file back over its `Platform/docs/` original so the
two stay identical. Every other file is reference only.

Drift check (prints nothing when every copy still equals its original):

```bash
cd docs/platform && while read -r sum path; do
  [ "$(sha256sum "/mnt/Developer/Artisan_Hosting/Platform/docs/$path" | cut -d' ' -f1)" = "$sum" ] || echo "changed: $path"
done < <(awk -F' [|] ' '/^[|] `/{gsub(/[` |]/,"",$1); gsub(/[` |]/,"",$3); print $3, $1}' README.md)
```

| Source file (`Platform/docs/...`) | Why the plans cite it | sha256 at copy time |
|---|---|---|
| `architecture/RESOURCE_TAXONOMY.md` | Normative spec for ids, URNs, status, naming (plan 01; plan 04 D21) | `1d4dd89b3eb1d5ada9b8ec63118b6cafdf09041b5818e95422be962f925aec18` |
| `architecture/PLATFORM_OVERVIEW.md` | Stale URN examples to fix (W2.10) | `d845d508dacf959ea24ca47960c049c5c756129358efe458c08718e685aba7b7` |
| `api/PORTAL_API.md` | §6 #10/#11/#13: the items deferred to v8.0.0; usage routes (§3.7) | `1813eb9ae338fc35a770226ad38ee7be8c97fadea4d9dc5c8f573ce9389747af` |
| `rbac/RBAC_IMPLEMENTATION_OVERVIEW.md` | Evaluator contract, fail-closed rule, `Action`/`ResourceType` vocabulary | `a4e8e9066beb8cc5ca79bf09756d44f51967612dd210ba3736886589a17f7656` |
| `rbac/RBAC_PHASE_1_CORE_ENGINE.md` | §6 `rbac_client` helper, §7 audit logging (W6) | `557596cad91fb01b5fe2f8704c069fd47859ffb2e3ceb70352fbd8b97541630b` |
| `rbac/RBAC_PHASE_2_MTLS.md` | mTLS material loader and node identity direction (W9 `identity.rs` TODO) | `c5713e4dfadbd22547b2c5484e39067a6d5648a8f55ba002648d5f28d183574b` |
| `notes/agent_eng.md` | House rules: pre-GA breaking changes, fail closed, path overrides, test flakiness | `46f2b1e465e3c07086a33523eecc68c2607ca3049d8f14cd93ff64b14ed739e9` |
| `notes/domain_billing_rollout.md` | Billing's replacement of `calculate_costs` (W3.8) | `1fdf6082ba38ef6afc4876f9f62c90db16b2828566ac92cd4d8f37392553879d` |
| `plans/per-app-2026-10/README.md` | Customer-deploy `CD-*` tasks that overlap the supervisor and config work | `74eeb2aebb0dd1f673681ab1e409177d3b85851ec418863b37c6fea11c6ffe5c` |
| `plans/per-app-2026-10/watchdog/CONTRACTS.md` | C2 policy file, C17 sandbox cgroup (W4.3, W5 limits) | `f2b7992b85aa813b4ca7e3db8b623fbc31f3c0a36b0aef57848b3135d56ab292` |
| `plans/per-app-2026-10/watchdog/tasks/SBX-007b.md` | Converts cgroup counters back to a percent (W4.4) | `f40b31155b4f6e65a6856e270a7635101fb24b89c69797082af477cc3f97894f` |
| `plans/per-app-2026-10/watchdog/tasks/CD-WD-1.md` | Port allocator behind `run.port = "auto"` (W5.2) | `cf9107bf988b5517fa6e7426d7540ad77f5f3b7a3e74607b844307ed03117196` |
| `plans/per-app-2026-10/gitmonitor/tasks/NODE-GIT-001.md` | Credential resolver that replaces `GitAuth.token` (W9) | `0526eb62c8ab23f242e5cd1f372eb9553b0541ec593f2bf47a31e0a2a8fdb563` |
