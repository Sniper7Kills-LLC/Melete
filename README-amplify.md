# Journal — Amplify Gen 2 backend

Amplify Gen 2 (`@aws-amplify/backend`, `ampx`) backend for Phase 6.3 template sharing.
Authors the Cognito user pool, AppSync GraphQL API, three DynamoDB tables
(`PageTemplate`, `NotebookTemplate`, `Brush`) and the S3 bucket that holds binary
template assets.

## Layout

```
amplify/
├── backend.ts                      # defineBackend({ auth, data, storage, assetPresign })
├── tsconfig.json
├── auth/resource.ts                # Cognito email login, EMAIL_ONLY recovery, no MFA
├── data/
│   ├── resource.ts                 # schema + 7 custom mutations
│   ├── fork-page-template.ts       # AppSync JS resolver
│   ├── fork-notebook-template.ts
│   ├── fork-brush.ts
│   ├── publish-page-template.ts
│   ├── publish-notebook-template.ts
│   └── publish-brush.ts
├── storage/resource.ts             # public/templates/* (read) + protected/{id}/templates/* (rwd)
└── functions/asset-presign/        # presign Lambda (sha256-validated PUT URLs)
```

## Sandbox workflow

`npx ampx sandbox` provisions a per-developer ephemeral stack and writes
`amplify_outputs.json` at the repo root. The Rust client `include_str!`s that file
at build time (Phase 6.3 issue follow-up).

```bash
npm install                # one-time
npx ampx sandbox            # provisions; idle cost ~$0–5/mo per sandbox
npx ampx sandbox delete     # tear down
```

The first `sandbox` run requires:

- AWS CLI creds with permission to provision Cognito/AppSync/DynamoDB/Lambda/S3 in
  the chosen region.
- An AWS account that has been bootstrapped for CDK
  (`npx cdk bootstrap aws://<account>/<region>` once per account/region).

## Cost note

Sandboxes run on-demand DynamoDB + AppSync (per-request) + a small Lambda; idle
cost is dominated by S3 storage of any uploaded test assets. Tear the sandbox
down (`npx ampx sandbox delete`) when not iterating.

**Do not deploy to a shared / prod environment from this scaffold.** The
`publishPageTemplate` resolver currently flips visibility metadata only — the
S3-CopyObject step that promotes assets from `protected/.../` to `public/.../`
is stubbed (`TODO(sandbox-creds)` in `publish-page-template.ts`). Wire that
follow-up before any non-sandbox deploy.

## Verifying this branch

No AWS creds are needed for the type-check loop:

```bash
npm install
npx tsc --noEmit -p amplify/tsconfig.json
```

The type-check is the gate that this scaffold ships green.
