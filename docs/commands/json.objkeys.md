Return the keys in the object that's referenced by `path`

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

JSON.OBJKEYS returns an array of array replies for each path, an array of the key names in the object as a bulk string reply, or `nil` if the matching JSON value is not an object. 
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":[3], "nested": {"a": {"b":2, "c": 1}}}'
OK
127.0.0.1:6379> JSON.OBJKEYS doc $..a
1) (nil)
2) 1) "b"
   2) "c"
{{< / highlight >}}

## See also

`JSON.ARRINDEX` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
