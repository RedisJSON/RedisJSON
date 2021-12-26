# RedisJSON commands

## Overview

### Supported JSON

RedisJSON aims to provide full support for [ECMA-404 The JSON Data Interchange Standard](http://json.org/).

Below, the term _JSON Value_ refers to any of the valid values. A _Container_ is either a _JSON Array_ or a _JSON Object_. A _JSON Scalar_ is a _JSON Number_, a _JSON String_, or a literal (_JSON False_, _JSON True_, or _JSON Null_).

### RedisJSON API

The module's commands are described below. Each section
header shows the syntax for the command, where:

*   Command and subcommand names are in uppercase, for example `JSON.SET` or `INDENT`
*   Mandatory arguments are enclosed in angle brackets, for example `<path>`
*   Optional arguments are enclosed in square brackets, for example `[index]`
*   Additional optional arguments are indicated by three period characters, for example `...`
*   The pipe character, `|`, means an exclusive or

Commands usually require a key's name as their first argument. The [path](path.md) is generally assumed to be the root if not specified.

The time complexity of the command does not include that of the [path](path.md#time-complexity-of-path-evaluation). The size - usually denoted _N_ - of a value is:

*   1 for scalar values
*   The sum of sizes of items in a container

## Scalar commands

### JSON.SET

> **Available since 1.0.0.**  
> **Time complexity:**  O(M+N) when path is evaluated to a single value, where M is the size of the original value (if it exists) and N is the size of the new value. O(M+N) when path is evaluated to multiple values, where M is the size of the key and N is the size of the new value.

#### Syntax

```sql
JSON.SET <key> <path> <json>
         [NX | XX]
```

#### Description

Sets the JSON value at `path` in `key`.

For new Redis keys, the `path` must be the root. For existing keys, replaces the current value with the `json` value if the entire `path` exists.

Adds a key (with its respective value) to a JSON Object (in a RedisJSON data type key) only if it is the last child in the `path`. The optional subcommands modify this behavior for both new RedisJSON data type keys as well as the JSON Object keys in them:

*   `NX` - only set the key if it does not already exist
*   `XX` - only set the key if it already exists

#### Return value

[Simple String][1] - `OK` if executed correctly, or [Null Bulk][3] if the specified `NX` or `XX`
conditions were not met.

### JSON.GET

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.GET <key>
         [INDENT indentation-string]
         [NEWLINE line-break-string]
         [SPACE space-string]
         [path ...]
```

#### Description

Returns the value at `path` in JSON serialized form.

This command accepts multiple `path` arguments. If no path is given, it defaults to the value's root.

The following subcommands change the reply's format (all are empty string by default):

*   `INDENT` sets the indentation string for nested levels
*   `NEWLINE` sets the string that's printed at the end of each line
*   `SPACE` sets the string that's put between a key and a value

Produce pretty-formatted JSON with `redis-cli` by following this example:

```
~/$ redis-cli --raw
127.0.0.1:6379> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
```

#### Return value

[Array][4] of [Bulk Strings][3] - each string is the JSON serialization of each JSON value that matches a path.

When using a JSONPath, the root of the matching values is always an array. In contrast, the legacy path returns a single value.

If there are multiple paths that include both legacy path and JSONPath, the returned value conforms to the JSONPath version (an array of values). 

#### Examples:

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":2, "b": 3, "nested": {"a": 4, "b": null}}'
OK
```

With a single JSONPath (JSON array bulk string):

```sql
127.0.0.1:6379> JSON.GET doc $..b
"[3,null]"
```

Using multiple paths with at least one JSONPath (map with array of JSON values per path):

```sql
127.0.0.1:6379> JSON.GET doc ..a $..b
"{\"$..b\":[3,null],\"..a\":[2,4]}"
```

### JSON.MGET

> **Available since 1.0.0.**  
> **Time complexity:**  O(M*N) when path is evaluated to a single value, where M is the number of keys and N is the size of the value. O(N1+N2+...+Nm) when path is evaluated to multiple values, where m is the number of keys and Ni is the size of the i-th key.

#### Syntax

```sql
JSON.MGET <key> [key ...] <path>
```

#### Description

Returns the values at `path` from multiple `key` arguments. Returns null for nonexistent keys and nonexistent paths.

#### Return value

[Array][4] of [Bulk Strings][3] - the JSON serialization of the value at each key's
path.

#### Example

Given the following documents:

```sql
127.0.0.1:6379> JSON.SET doc1 $ '{"a":1, "b": 2, "nested": {"a": 3}, "c": null}'
OK
127.0.0.1:6379> JSON.SET doc2 $ '{"a":4, "b": 5, "nested": {"a": 6}, "c": null}'
OK
```

```sql
127.0.0.1:6379> JSON.MGET doc1 doc2 $..a
1) "[1,3]"
2) "[4,6]"
```

### JSON.DEL

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the deleted value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.DEL <key> [path]
```

#### Description

Deletes a value.

`path` defaults to root if not provided. Ignores nonexistent keys and paths. Deleting an object's root is equivalent to deleting the key from Redis.

#### Return value

[Integer][2] - the number of paths deleted (0 or more).

#### Example

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a": 1, "nested": {"a": 2, "b": 3}}'
OK
127.0.0.1:6379> JSON.DEL doc $..a
(integer) 2
```

### JSON.CLEAR

> **Available since 2.0.0.**  
> **Time complexity:**  O(N), where N is the number of cleared values

#### Syntax

```
JSON.CLEAR <key> [path]
```

#### Description

Clear a container value (Array/Object).

`path` defaults to root if not provided. Non-existing keys and paths are ignored.

#### Return value

[Integer][2], specifically the number of containers cleared.

### JSON.NUMINCRBY

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.NUMINCRBY <key> <path> <number>
```

#### Description

Increments the number value stored at `path` by `number`.

#### Return value

[Bulk String][3] - the stringified new value for each path, or [null][6] if the matching JSON value is not a number.

#### Example

```sql
127.0.0.1:6379> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
127.0.0.1:6379> JSON.NUMINCRBY doc $.a 2
"[null]"
127.0.0.1:6379> JSON.NUMINCRBY doc $..a 2
"[null,4,7,null]"
```

### JSON.NUMMULTBY

> **Deprecated - will be dropped in the next version**  
> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

#### Syntax

```sql
JSON.NUMMULTBY <key> <path> <number>
```

#### Description

Multiplies the number value stored at `path` by `number`.

#### Return value

[Bulk String][3] - the stringified new values for each path, or [null][6] element if the matching JSON value is not a number.

#### Example

```sql
127.0.0.1:6379> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
127.0.0.1:6379> JSON.NUMMULTBY doc $.a 2
"[null]"
127.0.0.1:6379> JSON.NUMMULTBY doc $..a 2
"[null,4,10,null]"
```

### JSON.TOGGLE

> **Available since 2.0.0.**  
> **Time complexity:**  O(1).

#### Syntax

```
JSON.TOGGLE <key> <path>
```

#### Description

Toggle a boolean value stored at `path`.

#### Return value

[Integer][2], specifically the new value (0-false or 1-true), or [null][6] element for JSON values matching the path which are not boolean.

### JSON.STRAPPEND

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.STRAPPEND <key> [path] <json-string>
```

#### Description

Appends the `json-string` values to the string at `path`.

`path` defaults to root if not provided.

#### Return value

[Array][4] of [Integers][2] - for each path, the string's new length, or [null][6] if the matching JSON value is not an array.

#### Example

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":"foo", "nested": {"a": "hello"}, "nested2": {"a": 31}}'
OK
127.0.0.1:6379> JSON.STRAPPEND doc $..a '"baz"'
1) (integer) 6
2) (integer) 8
3) (nil)
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":\"foobaz\",\"nested\":{\"a\":\"hellobaz\"},\"nested2\":{\"a\":31}}]"
```

### JSON.STRLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.STRLEN <key> [path]
```

#### Description

Reports the length of the JSON String at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

#### Return value

[Array][4] of [Integers][2] - for each path, the string's length, or [null][6] if the matching JSON value is not a string.


#### Example

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":"foo", "nested": {"a": "hello"}, "nested2": {"a": 31}}'
OK
127.0.0.1:6379> JSON.STRLEN doc $..a
1) (integer) 3
2) (integer) 5
3) (nil)
```

## Array commands

### JSON.ARRAPPEND

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRAPPEND <key> <path> <json> [json ...]
```

#### Description

Append the `json` values into the array at `path` after the last element in it.

#### Return value

[Array][4] of [Integers][2] - for each path, the array's new size, or [null][6] if the matching JSON value is not an array.

#### Example

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1], "nested": {"a": [1,2]}, "nested2": {"a": 42}}'
OK
127.0.0.1:6379> JSON.ARRAPPEND doc $..a 3 4
1) (integer) 3
2) (integer) 4
3) (nil)
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":[1,3,4],\"nested\":{\"a\":[1,2,3,4]},\"nested2\":{\"a\":42}}]"
```

### JSON.ARRINDEX

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the array. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRINDEX <key> <path> <json-scalar> [start [stop]]
```

Searches for the first occurrence of a scalar JSON value in an array.

The optional inclusive `start` (default 0) and exclusive `stop` (default 0, meaning that the last element is included) specify a slice of the array to search.
Negative values are interpreted as starting from the end.


Note: out-of-range indexes round to the array's start and end. An inverse index range (such as the range from 1 to 0) will return unfound.

#### Return value

[Array][4] of [Integers][2] - the first position in the array of each JSON value that matches the path, -1 if unfound in the array, or [null][6] if the matching JSON value is not an array.

#### Examples

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": [3,4]}}'
OK
127.0.0.1:6379> JSON.ARRINDEX doc $..a 2
1) (integer) 1
2) (integer) -1
```

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
127.0.0.1:6379> JSON.ARRINDEX doc $..a 2
1) (integer) 1
2) (nil)
```

### JSON.ARRINSERT

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the array. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRINSERT <key> <path> <index> <json> [json ...]
```

#### Description

Inserts the `json` values into the array at `path` before the `index` (shifts to the right).

The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative index values start from the end of the array.

#### Return value

[Array][4] of [Integers][2] - for each path, the array's new size, or [null][6] if the matching JSON value is not an array.

#### Examples

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
127.0.0.1:6379> JSON.ARRINSERT doc $..a 0 1 2
1) (integer) 3
2) (integer) 4
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":[1,2,3],\"nested\":{\"a\":[1,2,3,4]}}]"
```

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
127.0.0.1:6379> JSON.ARRINSERT doc $..a 0 1 2
1) (integer) 6
2) (nil)
```

### JSON.ARRLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) where path is evaluated to a single value. O(N) where path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRLEN <key> [path]
```

Reports the length of the JSON Array at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

#### Return value

[Array][4] of [Integers][2] - for each path, the array's length, or [null][6] if the matching JSON value is not an array.

#### Examples

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
127.0.0.1:6379> JSON.ARRLEN doc $..a
1) (integer) 1
2) (integer) 2
```

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
127.0.0.1:6379> JSON.ARRLEN doc $..a
1) (integer) 4
2) (nil)
```

### JSON.ARRPOP

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the array and the specified index is not the last element. O(1) when path is evaluated to a single value and the specified index is the last element. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRPOP <key> [path [index]]
```

#### Description

Removes and returns an element from the index in the array.

`path` defaults to root if not provided. `index` is the position in the array to start popping from (defaults to -1, meaning the last element). Out-of-range indexes round to their respective array ends. Popping an empty array returns null.

#### Return value

[Array][4] of [Bulk Strings][3] - for each path, the popped JSON value, or [null][6] if the matching JSON value is not an array.

#### Examples

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
127.0.0.1:6379> JSON.ARRPOP doc $..a
1) "3"
2) "4"
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":[],\"nested\":{\"a\":[3]}}]"
```

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":["foo", "bar"], "nested": {"a": false}, "nested2": {"a":[]}}'
OK
127.0.0.1:6379> JSON.ARRPOP doc $..a
1) "\"bar\""
2) (nil)
3) (nil)
```

### JSON.ARRTRIM

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the array. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.ARRTRIM <key> <path> <start> <stop>
```

#### Description

Trims an array so that it contains only the specified inclusive range of elements.

This command is extremely forgiving and using it with out-of-range indexes will not produce an error. There are a few differences between how RedisJSON v2.0 and legacy versions handle out-of-range indexes.

Behavior as of RedisJSON v2.0:

* If `start` is larger than the array's size or `start` > `stop`, returns 0 and an empty array. 
* If `start` is < 0, then start from the end of the array.
* If `stop` is larger than the end of the array, it will be treated like the last element.

#### Return value

[Array][4] of [Integers][2] - for each path, the array's new size, or [null][6] if the matching JSON value is not an array.

#### Examples

```sql
127.0.0.1:6379> JSON.ARRTRIM doc $..a 1 1
1) (integer) 0
2) (integer) 1
127.0.0.1:6379> JSON.GET doc $
"[{\"a\":[],\"nested\":{\"a\":[4]}}]"
```

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
127.0.0.1:6379> JSON.ARRTRIM doc $..a 1 1
1) (integer) 1
2) (nil)
```

## Object commands

### JSON.OBJKEYS

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the number of keys in the object. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.OBJKEYS <key> [path]
```

#### Description

Returns the keys in the object that's referenced by `path`.

`path` defaults to root if not provided. Returns null if the object is empty or either `key` or `path` do not exist.

#### Return value

[Array][4] of [Arrays][4] - for each path, an array of the key names in the object as [Bulk Strings][3], or [null][6] if the matching JSON value is not an object. 

#### Example

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":[3], "nested": {"a": {"b":2, "c": 1}}}'
OK
127.0.0.1:6379> JSON.OBJKEYS doc $..a
1) (nil)
2) 1) "b"
   2) "c"
```

### JSON.OBJLEN

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.OBJLEN <key> [path]
```

#### Description

Reports the number of keys in the JSON Object at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

#### Return value

[Integer][2] - the number of keys in the object.

## Module commands

### JSON.TYPE

> **Available since 1.0.0.**  
> **Time complexity:**  O(1) when path is evaluated to a single value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.TYPE <key> [path]
```

#### Description

Reports the type of JSON value at `path`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

#### Return value

[Array][4] of [Simple Strings][1] - for each path, the value's type.

#### Examples

```sql
127.0.0.1:6379> JSON.SET doc $ '{"a":2, "nested": {"a": true}, "foo": "bar"}'
OK
127.0.0.1:6379> JSON.TYPE doc $..foo
1) "string"
127.0.0.1:6379> JSON.TYPE doc $..a
1) "integer"
2) "boolean"
127.0.0.1:6379> JSON.TYPE doc $..dummy
(empty array)
```

### JSON.DEBUG

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the JSON value.

#### Syntax

```sql
JSON.DEBUG <subcommand & arguments>
```

#### Description

Reports information.

Supported subcommands are:

*   `MEMORY <key> [path]` - report a value's memory usage in bytes. `path` defaults to root if
    not provided.
*   `HELP` - reply with a helpful message

#### Return value

Depends on the subcommand used.

*   `MEMORY` returns an [integer][2] - the value's size in bytes
*   `HELP` returns an [array][4] - with the help message

### JSON.FORGET

An alias for [`JSON.DEL`](#jsondel).

### JSON.RESP

> **Available since 1.0.0.**  
> **Time complexity:**  O(N) when path is evaluated to a single value, where N is the size of the value. O(N) when path is evaluated to multiple values, where N is the size of the key.

#### Syntax

```sql
JSON.RESP <key> [path]
```

#### Description

Returns the JSON in `key` in [Redis Serialization Protocol (RESP)][5] form.

`path` defaults to root if not provided. This command uses the following mapping from JSON to RESP:

*   JSON Null maps to the [RESP Null Bulk String][5]
*   JSON `false` and `true` values map to [RESP Simple Strings][1]
*   JSON Numbers map to [RESP Integers][2] or [RESP Bulk Strings][3], depending on type
*   JSON Strings map to [RESP Bulk Strings][3]
*   JSON Arrays are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `[` followed by the array's elements
*   JSON Objects are represented as [RESP Arrays][4] in which the first element is the [simple string][1] `{`. Each successive entry represents a key-value pair as a two-entry [array][4] of [bulk strings][3].

#### Return value

[Array][4] - the JSON's RESP form as detailed.

[1]:  http://redis.io/topics/protocol#resp-simple-strings
[2]:  http://redis.io/topics/protocol#resp-integers
[3]:  http://redis.io/topics/protocol#resp-bulk-strings
[4]:  http://redis.io/topics/protocol#resp-arrays
[5]:  http://redis.io/topics/protocol
[6]:  https://redis.io/topics/protocol#null-elements-in-arrays
