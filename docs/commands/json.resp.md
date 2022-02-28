Returns the JSON in `key` in [Redis Serialization Protocol (RESP)][5] form.

`path` defaults to root if not provided. This command uses the following mapping from JSON to RESP:

*   JSON Null maps to the [RESP Null Bulk String][5]
*   JSON `false` and `true` values map to [RESP Simple Strings][1]
*   JSON Numbers map to [RESP Integers][2] or [RESP Bulk Strings][3], depending on type
*   JSON Strings map to [RESP Bulk Strings][3]
*   JSON Arrays are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `[` followed by the array's elements
*   JSON Objects are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `{`. Each successive entry represents a key-value pair as a two-entry [array][4] of [bulk strings][3].

@return

[Array][4] - the JSON's RESP form as detailed.
