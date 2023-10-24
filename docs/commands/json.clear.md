Clear container values (arrays/objects) and set numeric values to `0`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. Nonexisting paths are ignored.
</details>

## Return

JSON.CLEAR returns an integer reply specifying the number of matching JSON arrays and objects cleared + number of matching JSON numerical values zeroed.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

{{% alert title="Note" color="warning" %}}
 
Already cleared values are ignored for empty containers and zero numbers.

{{% /alert %}}

## Examples

<details open>
<summary><b>Clear container values and set numeric values to <code>0</code></b></summary>

Create a JSON document.

{{< highlight bash >}}
redis> JSON.SET doc $ '{"obj":{"a":1, "b":2}, "arr":[1,2,3], "str": "foo", "bool": true, "int": 42, "float": 3.14}'
OK
{{< / highlight >}}

Clear all container values. This returns the number of objects with cleared values.

{{< highlight bash >}}
redis> JSON.CLEAR doc $.*
(integer) 4
{{< / highlight >}}

Get the updated document. Note that numeric values have been set to `0`.

{{< highlight bash >}}
redis> JSON.GET doc $
"[{\"obj\":{},\"arr\":[],\"str\":\"foo\",\"bool\":true,\"int\":0,\"float\":0}]"
{{< / highlight >}}
</details>

## See also

`JSON.ARRINDEX` | `JSON.ARRINSERT` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)

