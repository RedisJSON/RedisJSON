Increment the number value stored at `path` by `number`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

<details open><summary><code>value</code></summary> 

is number value to increment. 
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`.
</details>

## Return 

JSON.NUMINCRBY returns a bulk string reply specified as a stringified new value for each path, or `nil`, if the matching JSON value is not a number. 
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec). 

## Examples

<details open>
<summary><b>Add a new color to a list of product colors</b></summary>

Create a document.

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
{{< / highlight >}}

Increment a value of `a` object by 2. The command fails to find a number and returns `null`.

{{< highlight bash >}}
127.0.0.1:6379> JSON.NUMINCRBY doc $.a 2
"[null]"
{{< / highlight >}}

Recursively find and increment a value of all `a` objects. The command increments numbers it finds and returns `null` for nonnumber values.

{{< highlight bash >}}
127.0.0.1:6379> JSON.NUMINCRBY doc $..a 2
"[null,4,7,null]"
{{< / highlight >}}

</details>

## See also

`JSON.ARRINDEX` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
