Multiply the number value stored at `path` by `number`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

<details open><summary><code>value</code></summary> 

is number value to multiply. 
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`.
</details>

## Return

JSON.NUMMULTBY returns a bulk string reply specified as a stringified new values for each path, or `nil` element if the matching JSON value is not a number.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
127.0.0.1:6379> JSON.NUMMULTBY doc $.a 2
"[null]"
127.0.0.1:6379> JSON.NUMMULTBY doc $..a 2
"[null,4,10,null]"
{{< / highlight >}}

## See also

`JSON.NUMINCRBY` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
