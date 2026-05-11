import { defineFunction } from '@aws-amplify/backend';

// Admin Cognito proxy. UserEntitlement doesn't carry the user's email
// (Cognito owns it), so the admin panel's "search by email" feature
// goes through this Lambda which calls Cognito `AdminListUsers` /
// `AdminGetUser` and returns trimmed user summaries.
export const adminSearchUsers = defineFunction({
  name: 'admin-search-users',
  entry: './handler.ts',
  timeoutSeconds: 15,
  resourceGroupName: 'data',
});
