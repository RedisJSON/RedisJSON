Search for the first occurrence of a JSON value in an array

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse.
</details>

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`.
</details>

<details open><summary><code>value</code></summary> 

is value to find its index in one or more arrays. 

{{% alert title="About using strings with JSON commands" color="warning" %}}
To specify a string as an array value to index, wrap the quoted string with an additional set of single quotes. Example: `'"silver"'`. For more detailed use, see [Examples](#examples).
{{% /alert %}}
</details>

## Optional arguments

<details open><summary><code>start</code></summary> 

is inclusive start value to specify in a slice of the array to search. Default is `0`. 
</details>


<details open><summary><code>stop</code></summary> 

is exclusive stop value to specify in a slice of the array to search, including the last element. Default is `0`. Negative values are interpreted as starting from the end.
</details>

{{% alert title="About out-of-range indexes" color="warning" %}}

Out-of-range indexes round to the array's start and end. An inverse index range (such as the range from 1 to 0) returns unfound or `-1`.
{{% /alert %}}

## Return value 

`JSON.ARRINDEX` returns an [array](/docs/reference/protocol-spec/#resp-arrays) of integer replies for each path, the first position in the array of each JSON value that matches the path, `-1` if unfound in the array, or `nil`, if the matching JSON value is not an array.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec). 

## Examples

<details open>
<summary><b>Find the specific place of a color in a list of product colors</b></summary>

Create a document for noise-cancelling headphones in black and silver colors.

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET item:1 $ '{"name":"Noise-cancelling Bluetooth headphones","description":"Wireless Bluetooth headphones with noise-cancelling technology","connection":{"wireless":true,"type":"Bluetooth"},"price":99.98,"stock":25,"colors":["black","silver"]}'
OK
{{< / highlight >}}

Add color `blue` to the end of the `colors` array. `JSON.ARRAPEND` returns the array's new size.

{{< highlight bash >}}
127.0.0.1:6379> JSON.ARRAPPEND item:1 $.colors '"blue"'
1) (integer) 3
{{< / highlight >}}

Return the new length of the `colors` array.

{{< highlight bash >}}
JSON.GET item:1
"{\"name\":\"Noise-cancelling Bluetooth headphones\",\"description\":\"Wireless Bluetooth headphones with noise-cancelling technology\",\"connection\":{\"wireless\":true,\"type\":\"Bluetooth\"},\"price\":99.98,\"stock\":25,\"colors\":[\"black\",\"silver\",\"blue\"]}"
{{< / highlight >}}

Get the list of colors for the product.

{{< highlight bash >}}
127.0.0.1:6379> JSON.GET item:1 '$.colors[*]'
"[\"black\",\"silver\",\"blue\"]"
{{< / highlight >}}

Insert two more colors after the second color. You now have five colors.

{{< highlight bash >}}
127.0.0.1:6379> JSON.ARRINSERT item:1 $.colors 2 '"yellow"' '"gold"'
1) (integer) 5
{{< / highlight >}}

Get the updated list of colors.

{{< highlight bash >}}
127.0.0.1:6379> JSON.GET item:1 $.colors
"[[\"black\",\"silver\",\"yellow\",\"gold\",\"blue\"]]"
{{< / highlight >}}

Find the place where color `silver` is located.

{{< highlight bash >}}
127.0.0.1:6379> JSON.ARRINDEX item:1 $..colors '"silver"'
1) (integer) 1
{{< / highlight >}}
</details>

## See also

`JSON.ARRAPPEND` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)

