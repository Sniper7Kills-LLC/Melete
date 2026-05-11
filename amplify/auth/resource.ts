import { defineAuth } from '@aws-amplify/backend';

// Admin role gating uses two Cognito groups, both checked by AppSync
// `@aws_auth(cognito_groups: ...)` rules. `admin` = read-only access
// to insights (user list, stats, audit log). `superadmin` = write
// access (grant tier, override caps, disable / delete users). You
// promote a user via the Cognito console (User pools → Users → Add to
// group) so an attacker who compromises an admin email still can't
// elevate themselves to write access without a second factor outside
// the app.
export const auth = defineAuth({
  loginWith: {
    email: true,
  },
  accountRecovery: 'EMAIL_ONLY',
  groups: ['admin', 'superadmin'],
});
