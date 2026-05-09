import { defineStorage } from '@aws-amplify/backend';

export const storage = defineStorage({
  name: 'journalTemplateAssets',
  access: (allow) => ({
    'public/templates/*': [
      allow.guest.to(['read']),
      allow.authenticated.to(['read']),
    ],
    'protected/{entity_id}/templates/*': [
      allow.entity('identity').to(['read', 'write', 'delete']),
    ],
  }),
});
