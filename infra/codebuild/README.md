# Desktop-app build pipeline (CodeBuild, #43)

Single CloudFormation stack provisions the AWS-native build pipeline
that replaces the (now-deleted) GitHub Actions `packaging.yml`.

## What it creates

- **S3 bucket** `melete-releases-<account-id>` (versioned, AES256,
  public access blocked). Holds every artefact + manifest.
- **CodeConnections** to GitHub (`melete-github`). Comes up in
  `PENDING_HANDSHAKE` — finish via the AWS Console once.
- **IAM role** scoped to CloudWatch Logs + the releases bucket +
  the CodeConnections token. Shared by every CodeBuild project.
- **SNS topic** `melete-build-status` (subscribe an email at deploy
  time).
- **CodeBuild projects:**
  - `melete-build-linux` — `aws/codebuild/standard:7.0` Linux,
    BUILD_GENERAL1_LARGE, privileged (Docker-in-Docker for the
    Arch makepkg step). Builds: melete-app binary, .deb, .rpm,
    AppImage, .pkg.tar.zst, .flatpak. Uploads to
    `s3://${bucket}/binaries/v${VERSION}/linux-x86_64/`.
  - `melete-build-windows` — `aws/codebuild/windows-base:2022-1.0`,
    BUILD_GENERAL1_MEDIUM. gvsbuild GTK4 prebuilt + cargo-wix MSI +
    portable zip.
  - `melete-build-macos` (**conditional**, see below) —
    `aws/codebuild/macos-arm-base:14` MAC_ARM, BUILD_GENERAL1_LARGE.
    Builds the .app bundle (dylibbundler vendored) + DMG.
  - `melete-build-manifest` — Linux SMALL. Manually triggered after
    the per-OS builds finish. Aggregates `SHA256SUMS`, writes
    `manifest.json` for the version, bumps the root `latest.json`.
- **Webhook triggers** on every project filter to `refs/tags/v*` so
  push to `main` doesn't kick a build.

## Prerequisites

- AWS account with quota for the CodeBuild compute types above.
- `melete` AWS profile configured locally (per the project's
  preference).
- (macOS only) Preview enrolment for AWS-provided ARM macOS compute.
  Request via AWS Support if `EnableMacOS=true` is rejected on stack
  creation. Alternative: provision a self-managed Mac mini as a
  CodeBuild custom environment image (out of scope for V1).

## Deploy

Two-step because CodeBuild rejects references to a connection that's
still in `PENDING_HANDSHAKE` — OAuth must complete before stack
create.

### 1. Create the source connection + finish OAuth

```
aws codeconnections create-connection \
  --connection-name melete-github \
  --provider-type GitHub \
  --profile melete --region us-east-1
```

Returns a `ConnectionArn`. Open the AWS Console → **Developer Tools
→ Settings → Connections → melete-github → Update pending connection**
→ click through the GitHub OAuth flow. The connection's status flips
to `AVAILABLE` once you authorize the AWS app on the GitHub side.

Verify:
```
aws codeconnections get-connection --connection-arn <ARN> \
  --profile melete --region us-east-1 \
  --query 'Connection.ConnectionStatus'
```

Should print `"AVAILABLE"`.

### 2. Deploy the stack

```
aws cloudformation deploy \
  --profile melete \
  --region us-east-1 \
  --stack-name melete-build \
  --template-file infra/codebuild/stack.yaml \
  --capabilities CAPABILITY_NAMED_IAM \
  --parameter-overrides \
    GitHubRepo=Sniper7Kills-LLC/Melete \
    GitHubConnectionArn=<ARN-from-step-1> \
    NotificationEmail=sniper7kills@gmail.com \
    EnableMacOS=false
```

`EnableMacOS=false` for the first deploy — flip to `true` after you
confirm preview access (otherwise the stack rolls back).

## Post-deploy

1. **Subscribe the SNS topic.** Already wired if you passed
   `NotificationEmail`; confirm the AWS-sent email.

2. **Smoke test.** Tag a test version:
   ```
   git tag v0.0.1-test && git push origin v0.0.1-test
   ```
   Watch the Linux + Windows projects in CodeBuild. Each should land
   artefacts at `s3://${bucket}/binaries/v0.0.1-test/<platform>/`.
   Run the manifest aggregator:
   ```
   aws codebuild start-build \
     --profile melete --region us-east-1 \
     --project-name melete-build-manifest \
     --environment-variables-override 'name=VERSION,value=0.0.1-test,type=PLAINTEXT'
   ```
   Then verify:
   ```
   aws s3 ls s3://${bucket}/binaries/v0.0.1-test/ --recursive
   aws s3 cp s3://${bucket}/latest.json - | jq
   ```

4. **CloudFront / domain wiring** lives in the separate
   `infra/releases/` stack (still pending — see #66). Until that
   deploys, artefacts are reachable only via signed S3 URLs.

## Teardown

```
# Empty the bucket first (versioned bucket — see AWS CLI docs for
# bulk version-aware deletion).
aws s3 rm s3://<bucket> --recursive --profile melete

aws cloudformation delete-stack \
  --profile melete \
  --region us-east-1 \
  --stack-name melete-build
```
