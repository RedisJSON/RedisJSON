Merge one or more `value` in a JSON document adding, updating, deleting and/or replacing a array at `path` in `key`

Compliance with Json Merge Patch [RFC7396](https://datatracker.ietf.org/doc/html/rfc7396)

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary>

is key to merge.
</details>

<details open><summary><code>path</code></summary>

is JSONPath to specify. Default is root `$`. For new Redis keys the `path` must be the root. For existing keys, when the entire `path` exists, the value that it contains is replaced with the `json` value. For existing keys, when the `path` exists, except for the last element, a new child is added with the `json` value.

</details>

<details open><summary><code>value</code></summary>

is value to set at the specified path. `null` value delete the path. An array as value will replace the previous array
</details>

## Return value

JSET.MERGE returns a simple string reply: `OK` if executed correctly or `error` if fails to set the new values

For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

The JSON.MERGE provide 4 different behaviours to merge changes on a given key: create unexistent path, update an existing path with a new value, delete a existing path and/or replace an array with a new array

<details open>
<summary><b>Create a unexistent path-value</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":2}'
OK
127.0.0.1:6379> JSON.MERGE doc $.b '8'
OK
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":2,\"b\":8}]"
{{< / highlight >}}

</details>

<details open>
<summary><b>Replace an existing value</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":2}'
OK
127.0.0.1:6379> JSON.MERGE doc $.a '3'
OK
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":3}]"
{{< / highlight >}}

</details>

<details open>
<summary><b>Deletion on existing value</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":2}'
OK
127.0.0.1:6379> JSON.MERGE doc $.a 'null'
OK
127.0.0.1:6379> JSON.GET doc $
"[{}]"
{{< / highlight >}}

</details>

<details open>
<summary><b>Replacing an Array</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a":[2,4,6,8]}'
OK
127.0.0.1:6379> JSON.MERGE doc $.a '[10,12]'
OK
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":[10,12]}]"
{{< / highlight >}}

</details>


<details open>
<summary><b>Merge changes in multi-paths</b></summary>

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"f1": {"a":1}, "f2":{"a":2}}'
OK
127.0.0.1:6379> JSON.GET doc
"{\"f1\":{\"a\":1},\"f2\":{\"a\":2}}"
127.0.0.1:6379> JSON.MERGE doc $ '{"f1": 'null', "f2":{"a":3, "b":4}, "f3":'[2,4,6]'}' 
OK
127.0.0.1:6379> JSON.GET doc
"{\"f2\":{\"a\":3,\"b\":4},\"f3\":[2,4,6]}"
{{< / highlight >}}

</details>

## See also

`JSON.GET` | `JSON.MGET` | | `JSON.MSET`

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)
