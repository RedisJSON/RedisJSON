# ReJSON commands

## Quick reference

*   [ReJSON data type commands](#rejson-data-type-commands)
    *   [`JSON.DEL`](#del) deletes a value
    *   [`JSON.GET`](#get) gets a value
    *   [`JSON.MGET`](#mget) gets a value from multiple ReJSON keys
    *   [`JSON.SET`](#set) sets a value
    *   [`JSON.TYPE`](#type) reports the type of a value
*   [Number operations](#number-operations)
    *   [`JSON.NUMINCRBY`](#numincrby) increments a number
    *   [`JSON.NUMMULTBY`](#nummultby) multiplies a number
*   [String operations](#string-operations)
    *   [`JSON.STRAPPEND`](#strappend) appends a string to a string
    *   [`JSON.STRLEN`](#strlen) reports a string's length
*   [Array operations](#array-operations)
    *   [`JSON.ARRAPPEND`](#arrappend) appends values to an array
    *   [`JSON.ARRINDEX`](#arrindex) searches for the first occurance of a value in an array
    *   [`JSON.ARRINSERT`](#arrinsert) inserts values in an array
    *   [`JSON.ARRLEN`](#arrlen) reports the array's length
    *   [`JSON.ARRTRIM`](#arrtrim) trims an array to contain only a range of elements
*   [Object operations](#object-operations)
    *   [`JSON.OBJKEYS`](#objkeys) returns the keys in an object
    *   [`JSON.OBJLEN`](#objlen) reports the number of keys in an object
*   [Other commands](#other-commands)
    *   [`JSON.MEMORY`](#memory) returns the memory usage of a ReJSON key
    *   [`JSON.RESP`](#resp) returns a JSON value using Redis Serialization Protocol

## JSON

ReJSON aims at providing full support for
[ECMA-404 The JSON Data Interchange Standard](http://json.org/).

In the below, the term _JSON Value_ refers to any of the valid values. A _Container_ is either a
_JSON Array_ or a _JSON Object_. A _JSON Scalar_ is a _JSON Number_, a _JSON String_ or a literal
(_JSON False_, _JSON True_ or _JSON Null_).

## Path

Since there does not exist a standard for path syntax, ReJSON implements its own. ReJSON's syntax is
a subset of common best practices.

Paths always begin at the root of a ReJSON value. The root is denoted by the period character (`.`).
For paths referencing the root's children, prefixing the path with the root is optional.

Dotted- and square-bracketed, single-or-double-quoted-child notation are both supported for object
keys, so the following paths all refer to _boo_, child of _foo_ under the root:

*   `.foo.bar`
*   `foo["bar"]`
*   `['foo']["bar"]`

Array elements are accessed by their index enclosed by a pair of square brackets. The index is
0-based, with 0 being the first element of the array, 1 being the next element and so on. These
offsets can also be negative numbers indicating indices starting at the end of the array. For
example, -1 is the last element in the array, -2 the penultimate, and so on.

### A note about JSON and path compatability

By definition a JSON key can be any valid JSON String. Paths, on the other hand, are traditionally
based on JavaScript's (and in Java in turn) variable naming conventions. Therefore, while it is
possible to have ReJSON store objects containing arbitrary key names, accessing these keys via a
path will only be possible if they respect these naming syntax rules:

1.  Names must begin with a letter, a dollar (`$`) or an underscore (`_`) character
2.  Names can contain letters, digits, dollar signs and underscores
3.  Names are sensitive, case-wise

### Time complexity of path evaluation

The complexity of searching (navigating to) an element in the path is made of:

1. Child level - every level along the path adds an additional search
2. Key search - O(N)<sup>&#8224;</sup>, where N is the number of keys in the parent object
3. Array search - O(1)

This means that the overall time complexity of searching a path is _O(N*M)_, where N is the depth
and M is the number of parent object keys.

<sup>&#8224;</sup> while this is acceptable for objects where N is small, access in larger ones can
be optimized. This is planned for a future version.

## ReJSON data type commands

### <a name="del" />`JSON.DEL <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the deleted value.

Delete the value at `path`.

Non-existing keys as well as non-existing paths are ignored. Deleting an object's root is
equivalent to deleting the key from Redis.

#### Return value

[Integer][2], specifically the number of paths deleted (0 or 1).

### <a name="get" />`JSON.GET <key> [INDENT indentation-string] [NEWLINE line-break-string] [SPACE space-string] [path ...]`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the value.

Return the value at `path` in JSON serialized form.

This command accepts multiple `path`s, and defaults to the value's root when none are given.

The following subcommands change the reply's and are all set to the empty string by default:
*   `INDENT` sets the indentation string for nested levels
*   `NEWLINE` sets the string that's printed at the end of each line
*   `SPACE` sets the string that's put between a key and a value

Pretty-formatted JSON is producable with `redis-cli` by following this example:

```
~/$ redis-cli --raw
127.0.0.1:6379> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
```

#### Return value

[Bulk String][3], specifically the JSON serialization.

The reply's structure depends on the on the number of paths. A single path results in the value
being itself is returned, whereas multiple paths are returned as a JSON object in which each path
is a key.

### <a name="mget" />`JSON.MGET <key> [key ...] <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(M*N), where M is the number of keys and N is the size of the value.

Returns the values at `path` from multiple `key`s. Non-existing keys and non-existing paths are
reported as null.

#### Return value

[Array][4] of [Bulk Strings][3], specifically the JSON serialization of the value at each key's
path.

### <a name="set" />`JSON.SET <key> <path> <json>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(M+N), where M is the size of the original value (if it exists) and N is
> the size of the new value.

Sets the JSON value at `path` in `key`

For new keys the `path` must be the root. For existing keys, when the entire `path` exists, the
value that it contains is replaced with the `json` value. A key (with its respective value) is
added to a JSON Object only if it is the last child in the `path`.

#### Return value

[Simple String][1]: `OK`.

### <a name="type" />`JSON.TYPE <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Report the type of JSON value at `path`.

If the key or path do not exist, null is returned.

Increments the number value stored at `path` by `number`.

#### Return value

[Simple String][1], specifically the type of value.

## Number operations

### <a name="numincrby" />`JSON.NUMINCRBY <key> <path> <number>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Increments the number value stored at `path` by `number`.

#### Return value

[Bulk String][3], specifically the stringified new value.

### <a name="nummultby" />`JSON.NUMMULTBY <key> <path> <number>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Multiplies the number value stored at `path` by `number`.

#### Return value
[Bulk String][3], specifically the stringified new value.

## String operations

### <a name="strappend" />`JSON.STRAPPEND <key> <path> <json-string>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the new string's length.

Append the `json-string` value(s) the string at `path`.

#### Return value
[Integer][2], specifically the string's new length.

### <a name="strlen" />`JSON.STRLEN <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Report the length of the JSON String at `path` in `key`.

If the `key` does not exist, null is returned

#### Return value

[Integer][2], specifically the string's length.

## Array operations

### <a name="arrappend" />`JSON.ARRAPPEND <key> <path> <json> [json ...]`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Append the `json` value(s) into the array at `path` after the last element in it.

#### Return value

[Integer][2], specifically the array's new size.

### <a name="arrindex" />`JSON.ARRINDEX <key> <path> <json-scalar> [start] [stop]`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

Search for the first occurance of a scalar JSON value in an array.

The optional inclusive `start` (default 0) and exclusive `stop` (default 0, meaning that the last
element is included) specify a slice of the array to search.

Note: out of range errors are treated by rounding the index to the array's start and end. An
inverse index range (e.g, from 1 to 0) will return unfound.

#### Return value

[Integer][2], specifically the position of the scalar value in the array or -1 if unfound.

### <a name="arrinsert" />`JSON.ARRINSERT <key> <path> <index> <json> [json ...]`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

Insert the `json` value(s) into the array at `path` before the `index` (shifts to the right).

The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative
index values are interpreted as starting from the end.

#### Return value

[Integer][2], specifically the array's new size.

### <a name="arrlen" />`JSON.ARRLEN <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Report the length of the JSON Array at `path` in `key`.

If the `key` does not exist, null is returned

#### Return value

[Integer][2], specifically the array's length.

### <a name="arrtrim" />`JSON.ARRTRIM <key> <path> <start> <stop>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the array's size.

Trim an array so that it contains only the specified inclusive range of elements.

This command is extremely forgiving and using it with out of range indexes will not produce an
error. If `start` is larger than the array's size or `start` > `stop`, the result will be an empty
array. If `start` is < 0 then it will be treated as 0. If end is larger than the end of the array,
it will be treated like the last element in it.

#### Return value

[Integer][2], specifically the array's new size.

## Object operations

### <a name="objkeys" />`JSON.OBJKEYS <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the number of keys in the object.

Return the keys in the object that's referenced by `path`.

If the object is empty, or either key or path do not exist then null is returned.

#### Return value

[Array][4], specifically the key names in the object as [Bulk Strings][3].

### <a name="objlen" />`JSON.OBJLEN <key> <path>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(1).

Report the number of keys in the JSON Object at `path` in `key`.

If the `key` does not exist, null is returned

#### Return value

[Integer][2], specifically the number of keys in the object.

## Other commands

### <a name="forget" />`JSON.FORGET <key> <path>`

This command is an alias for [`JSON.DEL`](#del).

### <a name="memory" />`JSON.MEMORY <key>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the JSON value.

Compute the size in bytes of a JSON value.

#### Return value

[Integer][2], specifically the size in bytes of the value.

### <a name="resp" />`JSON.RESP <key>`

> **Available since 1.0.0.**  
> **Time complexity:**  O(N), where N is the size of the JSON value.

Return the JSON in `key` in [Redis Serialization Protocol (RESP)][5].

This command uses the following mapping from JSON to RESP:
-   JSON Null is mapped to the [RESP Null Bulk String][5]
-   JSON `false` and `true` values are mapped to the respective [RESP Simple Strings][1]
-   JSON Numbers are mapped to [RESP Integers][2] or [RESP Bulk Strings][3], depending on type
-   JSON Strings are mapped to [RESP Bulk Strings][3]
-   JSON Arrays are represented as [RESP Arrays][4] in which first element is the
          [simple string][1] `[` followed by the array's elements
-   JSON Objects are represented as [RESP Arrays][4] in which first element is the
          [simple string][1] `{`. Each successive entry represents a key-value pair as a two-entries
          [array][4] of [bulk strings][3].

#### Return value

[Array][4], specifically the JSON's RESP form as detailed.

[1]:  http://redis.io/topics/protocol#resp-simple-strings
[2]:  http://redis.io/topics/protocol#resp-integers
[3]:  http://redis.io/topics/protocol#resp-bulk-strings
[4]:  http://redis.io/topics/protocol#resp-arrays
[5]:  http://redis.io/topics/protocol