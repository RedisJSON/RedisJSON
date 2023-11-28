Return the values at `path` from multiple `key` arguments

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse. Returns `null` for nonexistent keys.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Returns `null` for nonexistent paths.

</details>

## Return

JSON.MGET returns an array of bulk string replies specified as the JSON serialization of the value at each key's path.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Return the values at <code>path</code> from multiple <code>key</code> arguments</b></summary>

Create two JSON documents.

{{< highlight bash >}}
redis> JSON.SET doc1 $ '{"a":1, "b": 2, "nested": {"a": 3}, "c": null}'
OK
redis> JSON.SET doc2 $ '{"a":4, "b": 5, "nested": {"a": 6}, "c": null}'
OK
{{< / highlight >}}

Get values from all arguments in the documents.

{{< highlight bash >}}
redis> JSON.MGET doc1 doc2 $..a
1) "[1,3]"
2) "[4,6]"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.GET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
