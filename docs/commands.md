# RedisJSON Commands

## Overview

### Supported JSON

RedisJSON aims to provide full support for [ECMA-404 The JSON Data Interchange Standard](http://json.org/).

Below, the term _JSON Value_ refers to any of the valid values. A _Container_ is either a _JSON Array_ or a _JSON Object_. A _JSON Scalar_ is a _JSON Number_, a _JSON String_ or a literal (_JSON False_, _JSON True_ or _JSON Null_).

### RedisJSON API

Each of the module's commands is described below. Each section
header shows the syntax for the command, where:

*   Command and subcommand names are in uppercase, for example `JSON.SET` or `INDENT`
*   Mandatory arguments are enclosed in angle brackets, e.g. `<path>`
*   Optional arguments are enclosed in square brackets, e.g. `[index]`
*   Additional optional arguments are indicated by three period characters, i.e. `...`
*   The pipe character, `|`, means an exclusive or

Commands usually require a key's name as their first argument. The [path](path.md) is generally assumed to be the root if not specified.

The time complexity of the command does not include that of the [path](path.md#time-complexity-of-path-evaluation). The size - usually denoted _N_ - of a value is:

*   1 for scalar values
*   The sum of sizes of items in a container


## JSON.DEL

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the deleted value.

### Syntax

```
JSON.DEL <key> [path]
```

### Description

Delete a value.

`path` defaults to root if not provided. Non-existing keys and paths are ignored. Deleting an object's root is equivalent to deleting the key from Redis.

### Return value

[Integer][2], specifically the number of paths deleted (0 or 1).

## JSON.GET

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the value.

### Syntax

```
JSON.GET <key>
         [INDENT indentation-string]
         [NEWLINE line-break-string]
         [SPACE space-string]
         [path ...]
```

### Description

Return the value at `path` in JSON serialized form.

This command accepts multiple `path`s, and defaults to the value's root when none are given.

The following subcommands change the reply's format and are all set to the empty string by default:
*   `INDENT` sets the indentation string for nested levels
*   `NEWLINE` sets the string that's printed at the end of each line
*   `SPACE` sets the string that's put between a key and a value

Pretty-formatted JSON is producible with `redis-cli` by following this example:

```
~/$ redis-cli --raw
127.0.0.1:6379> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
```

### Return value

[Bulk String][3], specifically the JSON serialization.

The reply's structure depends on the number of paths. A single path results in the value itself being returned, whereas multiple paths are returned as a JSON object in which each path is a key.

## JSON.MGET

> **Available since 1.0.0.**  
> **Time complexity:**  O(M*N), where M is the number of keys and N is the size of the value.

### Syntax

```
JSON.MGET <key> [key ...] <path>
```

### Description

Returns the values at `path` from multiple `key`s. Non-existing keys and non-existing paths are reported as null.

### Return value

[Array][4] of [Bulk Strings][3], specifically the JSON serialization of the value at each key's
path.

## JSON.SET

> **Available since 1.0.0.**  
> **Time complexity:**  O(M+N), where M is the size of the original value (if it exists) and N is
> the size of the new value.

### Syntax

```
JSON.SET <key> <path> <json>
         [NX | XX]
```

### Description

Sets the JSON value at `path` in `key`

For new Redis keys the `path` must be the root. For existing keys, when the entire `path` exists, the value that it contains is replaced with the `json` value.

A key (with its respective value) is added to a JSON Object (in a Redis RedisJSON data type key) if and only if it is the last child in the `path`. The optional subcommands modify this behavior for both new Redis RedisJSON data type keys as well as the JSON Object keys in them:

*   `NX` - only set the key if it does not already exist
*   `XX` - only set the key if it already exists

### Return value

[Simple String][1] `OK` if executed correctly, or [Null Bulk][3] if the specified `NX` or `XX`
conditions were not met.

## JSON.TYPE

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.TYPE <key> [path]
```

### Description

Report the type of JSON value at `path`.

`path` defaults to root if not provided. If the `key` or `path` do not exist, null is returned.

### Return value

[Simple String][1], specifically the type of value.

## JSON.NUMINCRBY

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.NUMINCRBY <key> <path> <number>
```

### Description

Increments the number value stored at `path` by `number`.

### Return value

[Bulk String][3], specifically the stringified new value.

## JSON.NUMMULTBY

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.NUMMULTBY <key> <path> <number>
```

### Description

Multiplies the number value stored at `path` by `number`.

### Return value

[Bulk String][3], specifically the stringified new value.

## JSON.STRAPPEND

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the new string's length.

### Syntax

```
JSON.STRAPPEND <key> [path] <json-string>
```

### Description

Append the `json-string` value(s) the string at `path`.

`path` defaults to root if not provided.

### Return value

[Integer][2], specifically the string's new length.

## JSON.STRLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.STRLEN <key> [path]
```

### Description

Report the length of the JSON String at `path` in `key`.

`path` defaults to root if not provided. If the `key` or `path` do not exist, null is returned.

### Return value

[Integer][2], specifically the string's length.

## JSON.ARRAPPEND

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.ARRAPPEND <key> <path> <json> [json ...]
```

### Description

Append the `json` value(s) into the array at `path` after the last element in it.

### Return value

[Integer][2], specifically the array's new size.

## JSON.ARRINDEX

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

### Syntax

```
JSON.ARRINDEX <key> <path> <json-scalar> [start [stop]]
```

Search for the first occurrence of a scalar JSON value in an array.

The optional inclusive `start` (default 0) and exclusive `stop` (default 0, meaning that the last element is included) specify a slice of the array to search.

Note: out of range errors are treated by rounding the index to the array's start and end. An inverse index range (e.g. from 1 to 0) will return unfound.

### Return value

[Integer][2], specifically the position of the scalar value in the array, or -1 if unfound.

## JSON.ARRINSERT

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

### Syntax

```
JSON.ARRINSERT <key> <path> <index> <json> [json ...]
```

### Description

Insert the `json` value(s) into the array at `path` before the `index` (shifts to the right).

The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative index values are interpreted as starting from the end.

### Return value

[Integer][2], specifically the array's new size.

## JSON.ARRLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.ARRLEN <key> [path]
```

Report the length of the JSON Array at `path` in `key`.

`path` defaults to root if not provided. If the `key` or `path` do not exist, null is returned.

### Return value

[Integer][2], specifically the array's length.

## JSON.ARRPOP

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size for `index` other than the last element,
> O(1) otherwise.

### Syntax

```
JSON.ARRPOP <key> [path [index]]
```

### Description

Remove and return element from the index in the array.

`path` defaults to root if not provided. `index` is the position in the array to start popping from (defaults to -1, meaning the last element). Out of range indices are rounded to their respective array ends. Popping an empty array yields null.

### Return value

[Bulk String][3], specifically the popped JSON value.

## JSON.ARRTRIM

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

### Syntax

```
JSON.ARRTRIM <key> <path> <start> <stop>
```

### Description

Trim an array so that it contains only the specified inclusive range of elements.

This command is extremely forgiving and using it with out of range indexes will not produce an error. If `start` is larger than the array's size or `start` > `stop`, the result will be an empty array. If `start` is < 0 then it will be treated as 0. If `stop` is larger than the end of the array, it will be treated like the last element in it.

### Return value

[Integer][2], specifically the array's new size.

## JSON.OBJKEYS

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the number of keys in the object.

### Syntax

```
JSON.OBJKEYS <key> [path]
```

### Description

Return the keys in the object that's referenced by `path`.

`path` defaults to root if not provided. If the object is empty, or either `key` or `path` do not exist, then null is returned.

### Return value

[Array][4], specifically the key names in the object as [Bulk Strings][3].

## JSON.OBJLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

### Syntax

```
JSON.OBJLEN <key> [path]
```

### Description

Report the number of keys in the JSON Object at `path` in `key`.

`path` defaults to root if not provided. If the `key` or `path` do not exist, null is returned.

### Return value

[Integer][2], specifically the number of keys in the object.

## JSON.DEBUG

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the JSON value.

### Syntax

```
JSON.DEBUG <subcommand & arguments>
```

### Description

Report information.

Supported subcommands are:

*   `MEMORY <key> [path]` - report the memory usage in bytes of a value. `path` defaults to root if
    not provided.
*   `HELP` - reply with a helpful message

### Return value

Depends on the subcommand used.

*   `MEMORY` returns an [integer][2], specifically the size in bytes of the value
*   `HELP` returns an [array][4], specifically with the help message

## JSON.FORGET

An alias for [`JSON.DEL`](#jsondel).

## JSON.RESP

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the JSON value.

### Syntax

```
JSON.RESP <key> [path]
```

### Description

Return the JSON in `key` in [Redis Serialization Protocol (RESP)][5].

`path` defaults to root if not provided. This command uses the following mapping from JSON to RESP:
-   JSON Null is mapped to the [RESP Null Bulk String][5]
-   JSON `false` and `true` values are mapped to the respective [RESP Simple Strings][1]
-   JSON Numbers are mapped to [RESP Integers][2] or [RESP Bulk Strings][3], depending on type
-   JSON Strings are mapped to [RESP Bulk Strings][3]
-   JSON Arrays are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `[` followed by the array's elements
-   JSON Objects are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `{`. Each successive entry represents a key-value pair as a two-entries [array][4] of [bulk strings][3].

### Return value

[Array][4], specifically the JSON's RESP form as detailed.

[1]:  http://redis.io/topics/protocol#resp-simple-strings
[2]:  http://redis.io/topics/protocol#resp-integers
[3]:  http://redis.io/topics/protocol#resp-bulk-strings
[4]:  http://redis.io/topics/protocol#resp-arrays
[5]:  http://redis.io/topics/protocol
