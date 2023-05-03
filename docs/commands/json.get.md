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

When using a single JSONPath, the root of the matching values is a JSON string with a top-level **array** of serialized JSON value. 
In contrast, a legacy path returns a single value.

When using multiple JSONPath arguments, the root of the matching values is a JSON string with a top-level **object**, with each object value being a top-level array of serialized JSON value.
In contrast, if all paths are legacy paths, each object value is a single serialized JSON value.
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
redis> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
{{< / highlight >}}

{{% /alert %}}

## Return

JSON.GET returns a bulk string representing a JSON array of string replies. 
Each string is the JSON serialization of each JSON value that matches a path. 
Using multiple paths, JSON.GET returns a bulk string representing a JSON object with string values. 
Each string value is an array of the JSON serialization of each JSON value that matches a path.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Return the value at <code>path</code> in JSON serialized form</b></summary>

Create a JSON document.

{{< highlight bash >}}
redis> JSON.SET doc $ '{"a":2, "b": 3, "nested": {"a": 4, "b": null}}'
OK
{{< / highlight >}}

With a single JSONPath (JSON array bulk string):

{{< highlight bash >}}
redis>  JSON.GET doc $..b
"[3,null]"
{{< / highlight >}}

Using multiple paths with at least one JSONPath returns a JSON string with a top-level object with an array of JSON values per path:

{{< highlight bash >}}
redis> JSON.GET doc ..a $..b
"{\"$..b\":[3,null],\"..a\":[2,4]}"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.MGET` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
