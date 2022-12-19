Set the JSON value at `path` in `key`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

<details open><summary><code>value</code></summary> 

is value to set at the specified path
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. For new Redis keys the `path` must be the root. For existing keys, when the entire `path` exists, the value that it contains is replaced with the `json` value. For existing keys, when the `path` exists, except for the last element, a new child is added with the `json` value. 

Adds a key (with its respective value) to a JSON Object (in a RedisJSON data type key) only if it is the last child in the `path`, or it is the parent of a new child being added in the `path`. Optional subcommands `NX` and `XX` modify this behavior for both new RedisJSON data type keys as well as the JSON Object keys in them.
</details>

<details open><summary><code>NX</code></summary> 

sets the key only if it does not already exist.
</details>

<details open><summary><code>XX</code></summary> 

sets the key only if it already exists.
</details>

## Return value 

JSET.SET returns a simple string reply: `OK` if executed correctly or `nil` if the specified `NX` or `XX` conditions were not met.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Replace an existing value</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":2}'
OK
127.0.0.1:6379> JSON.SET doc $.a '3'
OK
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":3}]"
{{< / highlight >}}
</details>

<details open>
<summary><b>Add a new value</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":2}'
OK
127.0.0.1:6379> JSON.SET doc $.b '8'
OK
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":2,\"b\":8}]"
{{< / highlight >}}
</details>

<details open>
<summary><b>Update multi-paths</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"f1": {"a":1}, "f2":{"a":2}}'
OK
127.0.0.1:6379> JSON.SET doc $..a 3
OK
127.0.0.1:6379> JSON.GET doc
"{\"f1\":{\"a\":3},\"f2\":{\"a\":3}}"
{{< / highlight >}}
</details>

## See also

`JSON.GET` | `JSON.MGET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
