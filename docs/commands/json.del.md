Delete a value

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to modify.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. Nonexisting paths are ignored.

{{% alert title="Note" color="warning" %}}
 
Deleting an object's root is equivalent to deleting the key from Redis.

{{% /alert %}}
</details>

## Return

JSON.DEL returns an integer reply specified as the number of paths deleted (0 or more).
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Delete a value</b></summary>

Create a JSON document.

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET doc $ '{"a": 1, "nested": {"a": 2, "b": 3}}'
OK
{{< / highlight >}}

Delete specified values.

{{< highlight bash >}}
127.0.0.1:6379> JSON.DEL doc $..a
(integer) 2
{{< / highlight >}}

Get the updated document.

{{< highlight bash >}}
127.0.0.1:6379> JSON.GET doc $
"[{\"nested\":{\"b\":3}}]"
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.ARRLEN` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)



