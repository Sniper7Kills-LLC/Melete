# Releases pipeline infrastructure (#66)

One-time AWS setup for the `release.yml` GitHub Actions workflow. The
workflow already exists at `.github/workflows/release.yml` and will
remain idle until the resources below are provisioned.

## What this stack creates

- **S3 bucket** `melete-releases-<account-id>` — versioned, SSE-AES256,
  public access blocked at the bucket level.
- **CloudFront distribution** that fronts the bucket via Origin Access
  Control (OAC). Backed by an ACM cert in `us-east-1` (mandatory for
  CloudFront).
- **Route 53 A record** for the chosen hostname (optional — only
  when `HostedZoneId` is supplied).
- **GitHub OIDC provider** for `token.actions.githubusercontent.com`
  (skipped when one already exists in the account).
- **IAM role** with trust policy restricted to tag pushes on the
  configured repo; permissions for `s3:PutObject`, `s3:GetObject`,
  `cloudfront:CreateInvalidation`.

## Prerequisites

1. Pick the hostname (default `releases.melete.app`).
2. Provision an ACM cert for that hostname in `us-east-1`. Validation
   is fastest via DNS records on the parent zone.
3. (Optional) Note the Route 53 hosted zone ID for the parent domain
   if you want this stack to wire DNS automatically.
4. (Optional) Check whether your AWS account already has the GitHub
   OIDC provider:
   ```
   aws iam list-open-id-connect-providers --profile melete \
     --query 'OpenIDConnectProviderList[?contains(Arn, `token.actions.githubusercontent.com`)]'
   ```
   Pass the ARN as `ExistingGitHubOidcProviderArn` if found.

## Deploy

```
aws cloudformation deploy \
  --profile melete \
  --region us-east-1 \
  --stack-name melete-releases \
  --template-file infra/releases/releases-stack.yaml \
  --capabilities CAPABILITY_NAMED_IAM \
  --parameter-overrides \
    AcmCertificateArn=arn:aws:acm:us-east-1:<account>:certificate/<id> \
    HostedZoneId=Z<your-zone-id> \
    ReleasesHostname=releases.melete.app
```

## Wire repo secrets

After the stack deploys, read the outputs:

```
aws cloudformation describe-stacks \
  --profile melete \
  --region us-east-1 \
  --stack-name melete-releases \
  --query 'Stacks[0].Outputs'
```

Set the four GitHub repo secrets from the outputs:

| Secret               | Source output            |
| -------------------- | ------------------------ |
| `AWS_RELEASE_ROLE_ARN` | `ReleaseRoleArn`       |
| `AWS_REGION`           | `us-east-1` (literal)  |
| `RELEASES_BUCKET`      | `ReleasesBucketName`   |
| `RELEASES_PUBLIC_URL`  | `ReleasesPublicUrl`    |

## Wire Amplify Hosting env

Add to the Amplify Hosting app's environment so the landing-page
"Download for Linux" CTA reads from the prod manifest:

```
VITE_RELEASES_MANIFEST_URL=https://releases.melete.app/latest.json
```

## Smoke test

```
git tag v0.0.1-test
git push origin v0.0.1-test
gh run watch
```

After the workflow succeeds, verify:

```
curl https://releases.melete.app/latest.json
```

The response should show `v0.0.1-test`, a `linux-x86_64` platform
entry, and a tarball URL that returns a 200.

## Teardown

```
aws s3 rm s3://<bucket> --recursive --profile melete   # versioned bucket — see CLI guide if many versions
aws cloudformation delete-stack \
  --profile melete \
  --region us-east-1 \
  --stack-name melete-releases
```
