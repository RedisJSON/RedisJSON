Toggle a Boolean value stored at `path`

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. 

</details>

## Return

JSON.TOGGLE returns an array of integer replies for each path, the new value (`0` if `false` or `1` if `true`), or `nil` for JSON values matching the path that are not Boolean.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Toogle a Boolean value stored at <code>path</code></b></summary>

Create a JSON document.

{{< highlight bash >}}
redis> JSON.SET doc $ '{"bool": true}'
OK
{{< / highlight >}}

Toggle the Boolean value.

{{< highlight bash >}}
redis> JSON.TOGGLE doc $.bool
1) (integer) 0
{{< / highlight >}}

Get the updated document.

{{< highlight bash >}}
redis> JSON.GET doc $
"[{\"bool\":false}]"
{{< / highlight >}}

Toggle the Boolean value.

{{< highlight bash >}}
redis> JSON.TOGGLE doc $.bool
1) (integer) 1
{{< / highlight >}}

Get the updated document.

{{< highlight bash >}}
redis> JSON.GET doc $
"[{\"bool\":true}]"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.GET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)

