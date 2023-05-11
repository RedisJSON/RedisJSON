Set or updates JSON values of one or more keys (multiple) for a given tripplet `key-path-value`

JSON.MSET is atomic, so all given keys are set at once or not. It is not possible for clients to see that some of the keys were updated while others are unchanged.

Important, JSON is a hierarquical structure if you change a value in specific path, subsequent nested values will be affected and may be unchanged


[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary>

is key to modify.
</details>

<details open><summary><code>path</code></summary>

is JSONPath to specify. Default is root `$`. For new Redis keys the `path` must be the root. For existing keys, when the entire `path` exists, the value that it contains is replaced with the `json` value. For existing keys, when the `path` exists, except for the last element, a new child is added with the `json` value.

</details>

<details open><summary><code>value</code></summary>

is value to set at the specified path
</details>



## Return value

JSET.MSET returns a simple string reply: `OK` if executed correctly or `error` if fails to set the new values

For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Add a new values in multiple keys</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.MSET doc1 $ '{"a":2}' doc2 $.f.a '3' doc3 $ '{"f1": {"a":1}, "f2":{"a":2}}'
OK
127.0.0.1:6379> JSON.GET doc1 $
"[{\"a\":2}]"
127.0.0.1:6379> JSON.GET doc2 $
"[{\"f\":{\"a\":3]"
127.0.0.1:6379> JSON.GET doc3
"{\"f1\":{\"a\":3},\"f2\":{\"a\":3}}"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.MGET` | `JSON.GET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
