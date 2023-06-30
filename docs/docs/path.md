---
title: "Path"
linkTitle: "Path"
weight: 3
description: Access specific elements within a JSON document
---

Paths help you access specific elements within a JSON document. Since no standard for JSON path syntax exists, Redis JSON implements its own. JSON's syntax is based on common best practices and intentionally resembles [JSONPath](http://goessner.net/articles/JsonPath/).

JSON supports two query syntaxes: [JSONPath syntax](#jsonpath-syntax) and the [legacy path syntax](#legacy-path-syntax) from the first version of JSON.

JSON knows which syntax to use depending on the first character of the path query. If the query starts with the character `$`, it uses JSONPath syntax. Otherwise, it defaults to the legacy path syntax.

The returned value is a JSON string with a top-level array of JSON serialized strings. 
And if multi-paths are used, the return value is a JSON string with a top-level object with values that are arrays of serialized JSON values.

## JSONPath support

JSON v2.0 introduced [JSONPath](http://goessner.net/articles/JsonPath/) support. It follows the syntax described by Goessner in his [article](http://goessner.net/articles/JsonPath/).

A JSONPath query can resolve to several locations in a JSON document. In this case, the JSON commands apply the operation to every possible location. This is a major improvement over [legacy path](#legacy-path-syntax) queries, which only operate on the first path.

Notice that the structure of the command response often differs when using JSONPath. See the [Commands](/commands/?group=json) page for more details.

The new syntax supports bracket notation, which allows the use of special characters like colon ":" or whitespace in key names.

If you want to include double quotes in your query, enclose the JSONPath within single quotes. For example:

```sh
JSON.GET store '$.inventory["headphones"]'
```

### JSONPath syntax

The following JSONPath syntax table was adapted from Goessner's [path syntax comparison](https://goessner.net/articles/JsonPath/index.html#e2).

| Syntax&nbsp;element | Description |
|----------------|-------------|
| $ | The root (outermost JSON element), starts the path. |
| . or [] | Selects a child element. |
| .. | Recursively descends through the JSON document. |
| * | Wildcard, returns all elements. |
| [] | Subscript operator, accesses an array element. |
| [,] | Union, selects multiple elements. |
| [start\:end\:step] | Array slice where start, end, and step are indexes. |
| ?() | Filters a JSON object or array. Supports comparison operators <nobr>(`==`, `!=`, `<`, `<=`, `>`, `>=`, `=~`)</nobr>, logical operators <nobr>(`&&`, `\|\|`)</nobr>, and parenthesis <nobr>(`(`, `)`)</nobr>. |
| () | Script expression. |
| @ | The current element, used in filter or script expressions. |

### JSONPath examples

The following JSONPath examples use this JSON document, which stores details about items in a store's inventory:

```json
{
   "inventory": {
      "headphones": [
         {
            "id": 12345,
            "name": "Noise-cancelling Bluetooth headphones",
            "description": "Wireless Bluetooth headphones with noise-cancelling technology",
            "wireless": true,
            "connection": "Bluetooth",
            "price": 99.98,
            "stock": 25,
            "free-shipping": false,
            "colors": ["black", "silver"]
         },
         {
            "id": 12346,
            "name": "Wireless earbuds",
            "description": "Wireless Bluetooth in-ear headphones",
            "wireless": true,
            "connection": "Bluetooth",
            "price": 64.99,
            "stock": 17,
            "free-shipping": false,
            "colors": ["black", "white"]
         },
         {
            "id": 12347,
            "name": "Mic headset",
            "description": "Headset with built-in microphone",
            "wireless": false,
            "connection": "USB",
            "price": 35.01,
            "stock": 28,
            "free-shipping": false
         }
      ],
      "keyboards": [
         {
            "id": 22345,
            "name": "Wireless keyboard",
            "description": "Wireless Bluetooth keyboard",
            "wireless": true,
            "connection": "Bluetooth",
            "price": 44.99,
            "stock": 23,
            "free-shipping": false,
            "colors": ["black", "silver"]
         },
         {
            "id": 22346,
            "name": "USB-C keyboard",
            "description": "Wired USB-C keyboard",
            "wireless": false,
            "connection": "USB-C",
            "price": 29.99,
            "stock": 30,
            "free-shipping": false
         }
      ]
   }
}
```

First, create the JSON document in your database:

```sh
JSON.SET store $ '{"inventory":{"headphones":[{"id":12345,"name":"Noise-cancelling Bluetooth headphones","description":"Wireless Bluetooth headphones with noise-cancelling technology","wireless":true,"connection":"Bluetooth","price":99.98,"stock":25,"free-shipping":false,"colors":["black","silver"]},{"id":12346,"name":"Wireless earbuds","description":"Wireless Bluetooth in-ear headphones","wireless":true,"connection":"Bluetooth","price":64.99,"stock":17,"free-shipping":false,"colors":["black","white"]},{"id":12347,"name":"Mic headset","description":"Headset with built-in microphone","wireless":false,"connection":"USB","price":35.01,"stock":28,"free-shipping":false}],"keyboards":[{"id":22345,"name":"Wireless keyboard","description":"Wireless Bluetooth keyboard","wireless":true,"connection":"Bluetooth","price":44.99,"stock":23,"free-shipping":false,"colors":["black","silver"]},{"id":22346,"name":"USB-C keyboard","description":"Wired USB-C keyboard","wireless":false,"connection":"USB-C","price":29.99,"stock":30,"free-shipping":false}]}}'
```

#### Access JSON examples

The following examples use the `JSON.GET` command to retrieve data from various paths in the JSON document.

You can use the wildcard operator `*` to return a list of all items in the inventory:

```sh
127.0.0.1:6379> JSON.GET store $.inventory.*
"[[{\"id\":12345,\"name\":\"Noise-cancelling Bluetooth headphones\",\"description\":\"Wireless Bluetooth headphones with noise-cancelling technology\",\"wireless\":true,\"connection\":\"Bluetooth\",\"price\":99.98,\"stock\":25,\"free-shipping\":false,\"colors\":[\"black\",\"silver\"]},{\"id\":12346,\"name\":\"Wireless earbuds\",\"description\":\"Wireless Bluetooth in-ear headphones\",\"wireless\":true,\"connection\":\"Bluetooth\",\"price\":64.99,\"stock\":17,\"free-shipping\":false,\"colors\":[\"black\",\"white\"]},{\"id\":12347,\"name\":\"Mic headset\",\"description\":\"Headset with built-in microphone\",\"wireless\":false,\"connection\":\"USB\",\"price\":35.01,\"stock\":28,\"free-shipping\":false}],[{\"id\":22345,\"name\":\"Wireless keyboard\",\"description\":\"Wireless Bluetooth keyboard\",\"wireless\":true,\"connection\":\"Bluetooth\",\"price\":44.99,\"stock\":23,\"free-shipping\":false,\"colors\":[\"black\",\"silver\"]},{\"id\":22346,\"name\":\"USB-C keyboard\",\"description\":\"Wired USB-C keyboard\",\"wireless\":false,\"connection\":\"USB-C\",\"price\":29.99,\"stock\":30,\"free-shipping\":false}]]"
```

For some queries, multiple paths can produce the same results. For example, the following paths return the names of all headphones:

```sh
127.0.0.1:6379> JSON.GET store $.inventory.headphones[*].name
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\",\"Mic headset\"]"
127.0.0.1:6379> JSON.GET store '$.inventory["headphones"][*].name'
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\",\"Mic headset\"]"
127.0.0.1:6379> JSON.GET store $..headphones[*].name
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\",\"Mic headset\"]"
```

The recursive descent operator `..` can retrieve a field from multiple sections of a JSON document. The following example returns the names of all inventory items:

```sh
127.0.0.1:6379> JSON.GET store $..name
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\",\"Mic headset\",\"Wireless keyboard\",\"USB-C keyboard\"]"
```

You can use an array slice to select a range of elements from an array. This example returns the names of the first two headphones:

```sh
127.0.0.1:6379> JSON.GET store $..headphones[0:2].name
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\"]"
```

Filter expressions `?()` let you select JSON elements based on certain conditions. You can use comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, and starting with version v2.4.2, also `=~`), logical operators (`&&`, `||`), and parenthesis (`(`, `)`) within these expressions. A filter expression can be applied on an array or on an object, iterating over all the **elements** in the array or all the **values** in the object, retrieving only the ones that match the filter condition. 

Paths within the filter condition are using the dot notation with either `@` to denote the current array element or the current object value, or `$` to denote the top-level element. For example, use `@.key_name` to refer to a nested value and `$.top_level_key_name` to refer to a top-level value.

Starting with version v2.4.2, the comparison operator `=~` can be used for matching a path of a string value on the left side against a regular expression pattern on the right side. For more information, see the [supported regular expression syntax docs](https://docs.rs/regex/latest/regex/#syntax).

Non-string values do not match. A match can only occur when the left side is a path of a string value and the right side is either a hard-coded string, or a path of a string value. See [examples](#json-filter-examples) below.

The regex match is partial, meaning `"foo"` regex pattern matches a string such as `"barefoots"`.
To make it exact, use the regex pattern `"^foo$"`.

Other JSONPath engines may use regex pattern between slashes, e.g., `/foo/`, and their match is exact.
They can perform partial matches using a regex pattern such as `/.*foo.*/`.

#### JSON Filter examples

In the following example, the filter only returns wireless headphones with a price less than 70:

```sh
127.0.0.1:6379> JSON.GET store $..headphones[?(@.price<70&&@.wireless==true)]
"[{\"id\":12346,\"name\":\"Wireless earbuds\",\"description\":\"Wireless Bluetooth in-ear headphones\",\"wireless\":true,\"connection\":\"Bluetooth\",\"price\":64.99,\"stock\":17,\"free-shipping\":false,\"colors\":[\"black\",\"white\"]}]"
```

This example filters the inventory for the names of items that support Bluetooth connections:

```sh
127.0.0.1:6379> JSON.GET store '$.inventory.*[?(@.connection=="Bluetooth")].name'
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\",\"Wireless keyboard\"]"
```

This example, starting with version v2.4.2, filters only keyboards with some sort of USB connection using regex match. Notice this match is case-insensitive thanks to the prefix `(?i)` in the regular expression pattern `"(?i)usb"`:

```sh
127.0.0.1:6379> JSON.GET store '$.inventory.keyboards[?(@.connection =~ "(?i)usb")]'
"[{\"id\":22346,\"name\":\"USB-C keyboard\",\"description\":\"Wired USB-C keyboard\",\"wireless\":false,\"connection\":\"USB-C\",\"price\":29.99,\"stock\":30,\"free-shipping\":false}]"
```
The regular expression pattern can also be specified using a path of a string value on the right side.

For example, let's add each keybaord object with a string value named `regex_pat`:

```sh
127.0.0.1:6379> JSON.SET store '$.inventory.keyboards[0].regex_pat' '"(?i)bluetooth"'
OK
127.0.0.1:6379> JSON.SET store '$.inventory.keyboards[1].regex' '"usb"'
OK
```

Now we can match against the value of `regex_pat` instead of a hard-coded regular expression pattern, and get the keyboard with the `Bluetooth` string in its `connection` key. Notice the one with `USB-C` does not match since its regular expression pattern is case-sensitive and the regular expression pattern is using lowercase:

```sh
127.0.0.1:6379> JSON.GET store '$.inventory.keyboards[?(@.connection =~ @.regex_pat)]'
"[{\"id\":22345,\"name\":\"Wireless keyboard\",\"description\":\"Wireless Bluetooth keyboard\",\"wireless\":true,\"connection\":\"Bluetooth\",\"price\":44.99,\"stock\":23,\"free-shipping\":false,\"colors\":[\"black\",\"silver\"],\"regex\":\"(?i)Bluetooth\",\"regex_pat\":\"(?i)bluetooth\"}]"
```

#### Update JSON examples

You can also use JSONPath queries when you want to update specific sections of a JSON document.

For example, you can pass a JSONPath to the `JSON.SET` command to update a specific field. This example changes the price of the first item in the headphones list:

```sh
127.0.0.1:6379> JSON.GET store $..headphones[0].price
"[99.98]"
127.0.0.1:6379> JSON.SET store $..headphones[0].price 78.99
"OK"
127.0.0.1:6379> JSON.GET store $..headphones[0].price
"[78.99]"
```

You can use filter expressions to update only JSON elements that match certain conditions. The following example changes `free-shipping` to `true` for any items with a price greater than 49:

```sh
127.0.0.1:6379> JSON.SET store $.inventory.*[?(@.price>49)].free-shipping true
"OK"
127.0.0.1:6379> JSON.GET store $.inventory.*[?(@.free-shipping==true)].name
"[\"Noise-cancelling Bluetooth headphones\",\"Wireless earbuds\"]"
```

JSONPath queries also work with other JSON commands that accept a path as an argument. For example, you can add a new color option for a set of headphones with `JSON.ARRAPPEND`:

```sh
127.0.0.1:6379> JSON.GET store $..headphones[0].colors
"[[\"black\",\"silver\"]]"
127.0.0.1:6379> JSON.ARRAPPEND store $..headphones[0].colors '"pink"'
1) "3"
127.0.0.1:6379> JSON.GET store $..headphones[0].colors
"[[\"black\",\"silver\",\"pink\"]]"
```

## Legacy path syntax

JSON v1 had the following path implementation. JSON v2 still supports this legacy path in addition to JSONPath.

Paths always begin at the root of a Redis JSON value. The root is denoted by a period character (`.`). For paths that reference the root's children, it is optional to prefix the path with the root.

JSON supports both dot notation and bracket notation for object key access. The following paths refer to _headphones_, which is a child of _inventory_ under the root:

*   `.inventory.headphones`
*   `inventory["headphones"]`
*   `['inventory']["headphones"]`

To access an array element, enclose its index within a pair of square brackets. The index is 0-based, with 0 being the first element of the array, 1 being the next element, and so on. You can use negative offsets to access elements starting from the end of the array. For example, -1 is the last element in the array, -2 is the second to last element, and so on.

### JSON key names and path compatibility

By definition, a JSON key can be any valid JSON string. Paths, on the other hand, are traditionally based on JavaScript's (and Java's) variable naming conventions.

Although JSON can store objects that contain arbitrary key names, you can only use a legacy path to access these keys if they conform to these naming syntax rules:

1.  Names must begin with a letter, a dollar sign (`$`), or an underscore (`_`) character
2.  Names can contain letters, digits, dollar signs, and underscores
3.  Names are case-sensitive

## Time complexity of path evaluation

The time complexity of searching (navigating to) an element in the path is calculated from:

1. Child level - every level along the path adds an additional search
2. Key search - O(N)<sup>&#8224;</sup>, where N is the number of keys in the parent object
3. Array search - O(1)

This means that the overall time complexity of searching a path is _O(N*M)_, where N is the depth and M is the number of parent object keys.

<sup>&#8224;</sup> While this is acceptable for objects where N is small, access can be optimized for larger objects.
