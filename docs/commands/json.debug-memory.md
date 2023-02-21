Report a value's memory usage in bytes 

[Examples](#examples)

## Required arguments

<details open><summary><code>key</code></summary> 

is key to parse.
</details>

## Optional arguments

<details open><summary><code>path</code></summary> 

is JSONPath to specify. Default is root `$`. 
</details>

## Return

JSON.DEBUG MEMORY returns an integer reply specified as the value size in bytes.
For more information about replies, see [Redis serialization protocol specification](/docs/reference/protocol-spec).

## Examples

<details open>
<summary><b>Report a value's memory usage in bytes</b></summary>

Create a JSON document.

{{< highlight bash >}}
127.0.0.1:6379> JSON.SET item:2 $ '{"name":"Wireless earbuds","description":"Wireless Bluetooth in-ear headphones","connection":{"wireless":true,"type":"Bluetooth"},"price":64.99,"stock":17,"colors":["black","white"], "max_level":[80, 100, 120]}'
OK
{{< / highlight >}}

Get the values' memory usage in bytes.

{{< highlight bash >}}
127.0.0.1:6379> JSON.DEBUG MEMORY item:2
(integer) 253
{{< / highlight >}}
</details>

## See also

`JSON.SET` | `JSON.ARRLEN` 

## Related topics

* [RedisJSON](/docs/stack/json)
* [Index and search JSON documents](/docs/stack/search/indexing_json)

