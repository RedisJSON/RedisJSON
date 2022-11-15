Return the value at `path` in JSON serialized form

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. JSON.GET accepts multiple `path` arguments.

{{% alert title="Note" color="warning" %}}

When using a JSONPath, the root of the matching values is always an array. In contrast, the legacy path returns a single value.
If there are multiple paths that include both legacy path and JSONPath, the returned value conforms to the JSONPath version (an array of values).

{{% /alert %}}

</details>

<details open><summary><code>INDENT</code></summary> 

sets the indentation string for nested levels.
</details>

<details open><summary><code>NEWLINE</code></summary> 

sets the string that's printed at the end of each line.
</details>

<details open><summary><code>SPACE</code></summary> 

sets the string that's put between a key and a value.
</details>

{{% alert title="Note" color="warning" %}}
 
Produce pretty-formatted JSON with `redis-cli` by following this example:

{{< highlight bash >}}
~/$ redis-cli --raw
127.0.0.1:6379> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
{{< / highlight >}}

{{% /alert %}}

## Return

JSON.GET returns an array of bulk string replies. Each string is the JSON serialization of each JSON value that matches a path.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Return the value at <code>path</code> in JSON serialized form</b></summary>

Create a JSON document.

{{< highlight bash >}}
127.0.0.1:6379>  JSON.SET doc $ '{"a":2, "b": 3, "nested": {"a": 4, "b": null}}'
OK
{{< / highlight >}}

With a single JSONPath (JSON array bulk string):

{{< highlight bash >}}
127.0.0.1:6379>  JSON.GET doc $..b
"[3,null]"
{{< / highlight >}}

Using multiple paths with at least one JSONPath (map with array of JSON values per path):

{{< highlight bash >}}
127.0.0.1:6379> JSON.GET doc ..a $..b
"{\"$..b\":[3,null],\"..a\":[2,4]}"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.MGET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
