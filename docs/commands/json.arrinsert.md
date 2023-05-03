Insert the `json` values into the array at `path` before the `index` (shifts to the right)

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

<details open><summary><code>value</code></summary> 

is one or more values to insert in one or more arrays. 

{{% alert title="About using strings with JSON commands" color="warning" %}}
To specify a string as an array value to insert, wrap the quoted string with an additional set of single quotes. Example: `'"silver"'`. For more detailed use, see [Examples](#examples).
{{% /alert %}}
</details>

<details open><summary><code>index</code></summary> 

is position in the array where you want to insert a value. The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative index values start from the end of the array.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`.
</details>

## Return value 

`JSON.ARRINSERT` returns an [array](/docs/reference/protocol-spec/#resp-arrays) of integer replies for each path, the array's new size, or `nil`, if the matching JSON value is not an array. 
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec). 

## Examples

<details open>
<summary><b>Add new colors to a specific place in a list of product colors</b></summary>

Create a document for noise-cancelling headphones in black and silver colors.

{{< highlight bash >}}
redis> JSON.SET item:1 $ '{"name":"Noise-cancelling Bluetooth headphones","description":"Wireless Bluetooth headphones with noise-cancelling technology","connection":{"wireless":true,"type":"Bluetooth"},"price":99.98,"stock":25,"colors":["black","silver"]}'
OK
{{< / highlight >}}

Add color `blue` to the end of the `colors` array. `JSON.ARRAPEND` returns the array's new size.

{{< highlight bash >}}
redis> JSON.ARRAPPEND item:1 $.colors '"blue"'
1) (integer) 3
{{< / highlight >}}

Return the new length of the `colors` array.

{{< highlight bash >}}
JSON.GET item:1
"{\"name\":\"Noise-cancelling Bluetooth headphones\",\"description\":\"Wireless Bluetooth headphones with noise-cancelling technology\",\"connection\":{\"wireless\":true,\"type\":\"Bluetooth\"},\"price\":99.98,\"stock\":25,\"colors\":[\"black\",\"silver\",\"blue\"]}"
{{< / highlight >}}

Get the list of colors for the product.

{{< highlight bash >}}
redis> JSON.GET item:1 '$.colors[*]'
"[\"black\",\"silver\",\"blue\"]"
{{< / highlight >}}

Insert two more colors after the second color. You now have five colors.

{{< highlight bash >}}
redis> JSON.ARRINSERT item:1 $.colors 2 '"yellow"' '"gold"'
1) (integer) 5
{{< / highlight >}}

Get the updated list of colors.

{{< highlight bash >}}
redis> JSON.GET item:1 $.colors
"[[\"black\",\"silver\",\"yellow\",\"gold\",\"blue\"]]"
{{< / highlight >}}
</details>

## See also

`JSON.ARRAPPEND` | `JSON.ARRINDEX` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
