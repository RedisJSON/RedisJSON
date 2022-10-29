Report the length of the JSON array at `path` in `key`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`, if not provided. Returns null if the `key` or `path` do not exist.
</details>

## Return

JSON.ARRLEN returns by recursive descent an array of integer replies for each path, the array's length, or `nil`, if the matching JSON value is not an array.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec). 

## Examples

<details open>
<summary><b>Get lengths of JSON arrays in a document</b></summary>

Create a document for wireless earbuds.

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET item:2 $ '{"name":"Wireless earbuds","description":"Wireless Bluetooth in-ear headphones","connection":{"wireless":true,"type":"Bluetooth"},"price":64.99,"stock":17,"colors":["black","white"], "max_level":[80, 100, 120]}'
OK
{{< / highlight >}}

Find lengths of arrays in all objects of the document.

{{< highlight bash >}}
127.0.0.1:6379> JSON.ARRLEN item:2 '$.[*]'
1) (nil)
2) (nil)
3) (nil)
4) (nil)
5) (nil)
6) (integer) 2
7) (integer) 3
{{< / highlight >}}

Return the length of the `max_level` array.

{{< highlight bash >}}
127.0.0.1:6379> JSON.ARRLEN item:2 '$..max_level'
1) (integer) 3
{{< / highlight >}}

Get all the maximum level values.

{{< highlight bash >}}
127.0.0.1:6379> JSON.GET item:2 '$..max_level'
"[[80,100,120]]"
{{< / highlight >}}

</details>

## See also

`JSON.ARRINDEX` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
