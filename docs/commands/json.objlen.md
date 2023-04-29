Report the number of keys in the JSON object at `path` in `key`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse. Returns `null` for nonexistent keys.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. Returns `null` for nonexistant path.

</details>

## Return

JSON.OBJLEN returns an array of integer replies for each path specified as the number of keys in the object or `nil`, if the matching JSON value is not an object.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

{{< highlight bash >}}
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": {"b":2, "c": 1}}}'
OK
redis> JSON.OBJLEN doc $..a
1) (nil)
2) (integer) 2
{{< / highlight >}}

## See also

`JSON.ARRINDEX` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
