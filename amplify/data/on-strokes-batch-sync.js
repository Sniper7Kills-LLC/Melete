// Custom subscription handler. AppSync requires a handler even for
// passthrough subscriptions wired via `for()`. NoneDataSource is the
// canonical no-op data source — request/response just thread the
// AppSync framework. The mutation's return value is the actual
// payload subscribers receive.

// Custom subscription handler — see comments in resource.ts. The
// subscribe-time response is a typed-zero stub satisfying the
// non-nullable fields on StrokesBatchResult; event-time payloads
// come from the source mutation via the .for() reference.
export function request(ctx) {
  return {
    payload: {
      notebookId: ctx.args?.notebookId ?? '',
      upserted: 0,
      unprocessed: 0,
      ids: [],
      failedIds: [],
    },
  };
}

export function response(ctx) {
  return ctx.result;
}
