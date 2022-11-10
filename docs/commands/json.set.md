Sets the JSON value at `path` in `key`.

For new Redis keys the `path` must be the root. For existing keys, when the entire `path` exists, the value that it contains is replaced with the `json` value. For existing keys, when the `path` exists, except for the last element, a new child is added with the `json` value. 

Adds a key (with its respective value) to a JSON Object (in a RedisJSON data type key) only if it is the last child in the `path`, or it is the parent of a new child being added in the `path`. The optional subcommands modify this behavior for both new RedisJSON data type keys as well as the JSON Object keys in them:

*   `NX` - only set the key if it does not already exist
*   `XX` - only set the key if it already exists

@return

@simple-string-reply - `OK` if executed correctly, or @nil-reply if the specified `NX` or `XX`
conditions were not met.

@examples

Replacing an existing value

```
redis> JSON.SET doc $ '{"a":2}'
OK
redis> JSON.SET doc $.a '3'
OK
redis> JSON.GET doc $
"[{\"a\":3}]"
```

Adding a new value

```
redis> JSON.SET doc $ '{"a":2}'
OK
redis> JSON.SET doc $.b '8'
OK
redis> JSON.GET doc $
"[{\"a\":2,\"b\":8}]"
```

Updating multi paths

```
redis> JSON.SET doc $ '{"f1": {"a":1}, "f2":{"a":2}}'
OK
redis> JSON.SET doc $..a 3
OK
redis> JSON.GET doc
"{\"f1\":{\"a\":3},\"f2\":{\"a\":3}}"
```
