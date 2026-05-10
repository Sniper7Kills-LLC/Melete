import { defineStorage } from '@aws-amplify/backend';

export const storage = defineStorage({
  name: 'journalTemplateAssets',
  access: (allow) => ({
    'public/templates/*': [
      allow.guest.to(['read']),
      allow.authenticated.to(['read']),
    ],
    // {entity_id} must sit immediately before the ending wildcard
    // (Amplify Gen 2 storage rule). The lambda + Rust client compose
    // the deeper `templates/{templateId}/assets/{sha256}` suffix
    // under this prefix; the wildcard covers it all.
    'protected/{entity_id}/*': [
      allow.entity('identity').to(['read', 'write', 'delete']),
    ],
  }),
});
