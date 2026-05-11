import {
  CognitoIdentityProviderClient,
  ListUsersCommand,
} from '@aws-sdk/client-cognito-identity-provider';
import type { Schema } from '../../data/resource';

interface Env {
  USER_POOL_ID: string;
}

const env = process.env as unknown as Env;
const cognito = new CognitoIdentityProviderClient({});

export const handler: Schema['adminSearchUsers']['functionHandler'] = async (
  event,
) => {
  // AppSync resolver bound to this Lambda enforces the admin /
  // superadmin group via `@aws_auth`; this check is belt-and-braces
  // in case a misconfigured deployment lets it through.
  const groups =
    event.identity && 'groups' in event.identity
      ? ((event.identity.groups ?? []) as string[])
      : [];
  if (!groups.includes('admin') && !groups.includes('superadmin')) {
    throw new Error('FORBIDDEN');
  }

  const email = (event.arguments.email ?? '').trim();
  if (!email) return { items: [] };

  const result = await cognito.send(
    new ListUsersCommand({
      UserPoolId: env.USER_POOL_ID,
      Filter: `email ^= "${email.replace(/"/g, '')}"`,
      Limit: 25,
    }),
  );

  const items = (result.Users ?? []).map((u) => {
    const attrs = Object.fromEntries(
      (u.Attributes ?? []).map((a) => [a.Name ?? '', a.Value ?? '']),
    );
    return {
      userId: attrs.sub ?? '',
      email: attrs.email ?? '',
      enabled: u.Enabled ?? true,
      status: u.UserStatus ?? '',
      createdAtIso: u.UserCreateDate
        ? u.UserCreateDate.toISOString()
        : null,
    };
  });

  return { items };
};
