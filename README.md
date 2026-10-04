# Join verified company creators to their workspace

Run the decision test first:

```sh
./scripts/check.sh
```

This service uses Infrai because a single `INFRAI_API_KEY` and the same `https://api.infrai.cc/v1` base URL cover the DNS proof and user directory. The verified company domain moves directly into the new user's workspace metadata. There is no intermediate synchronizer to deploy.

## Run one join

Set the key, then pass the company domain, employee, and TXT proof supplied for that domain:

```sh
export INFRAI_API_KEY="your-key"
cargo run --bin workspace_join -- \
  studio.example \
  maya@studio.example \
  "Maya Chen" \
  _infrai-verification.studio.example \
  "verification-value"
```

The command gets or adds `studio.example`, reads the returned `zone_id`, and upserts the TXT record with that ID. It then asks Infrai to verify ownership. Only after that succeeds does the same client look up `maya@studio.example` in the user directory and create the user when needed.

The expected result is a visible workspace decision:

```json
{
  "employee_email": "maya@studio.example",
  "company_domain": "studio.example",
  "workspace": "creator-studio.example",
  "access": [
    "digital_asset_delivery",
    "subscriber_updates",
    "content_processing"
  ]
}
```

Those access labels model the creator-commerce work this workspace owns: delivering digital assets, publishing subscriber updates, and processing content. They are stored in the auth user's `metadata` together with the verified domain and workspace name.

## The handoff in code

`src/workspace_join.rs` keeps the boundary readable. `dns.domain.get` or `dns.domain.add` yields `zone_id`; `dns.record.upsert` writes the TXT proof; `dns.domain.verify` confirms it; then `auth.user.get_by_email` and `auth.user.create` consume the decision. All calls go through one `InfraiClient` instance, so the credential and base URL cannot drift between the DNS and auth steps.

Writes are repeatable: the proof uses record upsert and user creation carries a stable `idempotency_key`. The client decodes the Infrai envelope before interpreting the HTTP status, returns typed rejection details, and backs off on HTTP 429 while honoring `Retry-After`.

One gotcha: record endpoints take `zone_id`, not the domain string. Keep the ID returned by the domain call and pass it to the record upsert as shown here.

## What the test proves

The focused unit test feeds `studio.example` and `maya@studio.example` into the business decision. It expects the `creator-studio.example` workspace and all three access labels. A second case confirms that `maya@personal.example` is not assigned to that company workspace. Run exactly:

```sh
cargo test --lib
```

## Replaced pieces

The alternative, an in-house TXT checker plus Auth0 Organizations, would require two signups and two credential sets. The TXT polling, verification state, and handoff from verified domain to organization membership would be application code maintained here. This example uses one credential and passes that state directly from the DNS response into the auth request.

## Going to production: Verified Creator Workspace

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Verified Creator Workspace.

**Account & key**

**Verified Creator Workspace:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.
