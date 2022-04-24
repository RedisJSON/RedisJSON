Returns the JSON in `key` in [Redis Serialization Protocol (RESP)][5] form.

`path` defaults to root if not provided. This command uses the following mapping from JSON to RESP:

*   JSON Null maps to the @bulk-string-reply
*   JSON `false` and `true` values map to @simple-string-reply
*   JSON Numbers map to @integer-reply or @bulk-string-reply, depending on type
*   JSON Strings map to @bulk-string-reply
*   JSON Arrays are represented as @array-reply in which the first element is the @simple-string-reply `[` followed by the array's elements
*   JSON Objects are represented as @array-reply in which the first element is the @simple-string-reply `{`. Each successive entry represents a key-value pair as a two-entry @array-reply of @bulk-string-reply.

@return

@array-reply - the JSON's RESP form as detailed.
