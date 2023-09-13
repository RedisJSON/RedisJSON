---
title: "Path"
linkTitle: "Path"
weight: 3
description: Access specific elements within a JSON document
aliases:
    - /docs/stack/json/path
---

Paths help you access specific elements within a JSON document. Since no standard for JSON path syntax exists, Redis JSON implements its own. JSON's syntax is based on common best practices and intentionally resembles [JSONPath](http://goessner.net/articles/JsonPath/).

JSON supports two query syntaxes: [JSONPath syntax](#jsonpath-syntax) and the [legacy path syntax](#legacy-path-syntax) from the first version of JSON.

JSON knows which syntax to use depending on the first character of the path query. If the query starts with the character `$`, it uses JSONPath syntax. Otherwise, it defaults to the legacy path syntax.

The returned value is a JSON string with a top-level array of JSON serialized strings. 
And if multi-paths are used, the return value is a JSON string with a top-level object with values that are arrays of serialized JSON values.

## JSONPath support

RedisJSON v2.0 introduced [JSONPath](http://goessner.net/articles/JsonPath/) support. It follows the syntax described by Goessner in his [article](http://goessner.net/articles/JsonPath/).

A JSONPath query can resolve to several locations in a JSON document. In this case, the JSON commands apply the operation to every possible location. This is a major improvement over [legacy path](#legacy-path-syntax) queries, which only operate on the first path.

Notice that the structure of the command response often differs when using JSONPath. See the [Commands](/commands/?group=json) page for more details.

The new syntax supports bracket notation, which allows the use of special characters like colon ":" or whitespace in key names.

If you want to include double quotes in your query, enclose the JSONPath within single quotes. For example:

```sh
JSON.GET store '$.inventory["mountain_bikes"]'
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
      "mountain_bikes": [
         {
            "id": "bike:1",
            "model": "Phoebe",
            "description": "This is a mid-travel trail slayer that is a fantastic daily driver or one bike quiver. The Shimano Claris 8-speed groupset gives plenty of gear range to tackle hills and there\u2019s room for mudguards and a rack too.  This is the bike for the rider who wants trail manners with low fuss ownership.",
            "price": 1920,
            "specs": {
               "material": "carbon", 
               "weight": 13.1
               },
            "colors": ["black", "silver"]
         },
         {
            "id": "bike:2",
            "model": "Quaoar",
            "description": "Redesigned for the 2020 model year, this bike impressed our testers and is the best all-around trail bike we\'ve ever tested. The Shimano gear system effectively does away with an external cassette, so is super low maintenance in terms of wear and tear. All in all it\'s an impressive package for the price, making it very competitive.",
            "price": 2072,
            "specs": {
               "material": "aluminium", 
               "weight": 7.9
               },
            "colors": ["black", "white"]
         },
         {
            "id": "bike:3",
            "model": "Weywot",
            "description": "This bike gives kids aged six years and older a durable and uberlight mountain bike for their first experience on tracks and easy cruising through forests and fields. A set of powerful Shimano hydraulic disc brakes provide ample stopping ability. If you\'re after a budget option, this is one of the best bikes you could get.",
            "price": 3264,
            "specs": {
               "material": "alloy", 
               "weight": 13.8
               }
         }
      ],
      "commuter_bikes": [
         {
            "id": "bike:4",
            "model": "Salacia",
            "description": "This bike is a great option for anyone who just wants a bike to get about on With a slick-shifting Claris gears from Shimano\u2019s, this is a bike which doesn\u2019t break the bank and delivers craved performance.  It\u2019s for the rider who wants both efficiency and capability.",
            "price": 1475,
            "specs": {
               "material": "aluminium", 
               "weight": 16.6
               },
            "colors": ["black", "silver"]
         },
         {
            "id": "bike:5",
            "model": "Mimas",
            "description": "A real joy to ride, this bike got very high scores in last years Bike of the year report. The carefully crafted 50-34 tooth chainset and 11-32 tooth cassette give an easy-on-the-legs bottom gear for climbing, and the high-quality Vittoria Zaffiro tires give balance and grip.It includes a low-step frame , our memory foam seat, bump-resistant shocks and conveniently placed thumb throttle. Put it all together and you get a bike that helps redefine what can be done for this price.",
            "price": 3941,
            "specs": {
               "material": "alloy", 
               "weight": 11.6
               }
         }
      ]
   }
}
```

First, create the JSON document in your database:

{{< clients-example json_tutorial set_bikes >}}
> JSON.SET "bikes:inventory" "$" "{\"mountain_bikes\": [{\"id\": \"bike:1\", \"model\": \"Phoebe\", \"description\": \"This is a mid-travel trail slayer that is a fantastic daily driver or one bike quiver. The Shimano Claris 8-speed groupset gives plenty of gear range to tackle hills and there\\u2019s room for mudguards and a rack too.  This is the bike for the rider who wants trail manners with low fuss ownership.\", \"price\": 1920, \"specs\": {\"material\": \"carbon\", \"weight\": 13.1}, \"colors\": [\"black\", \"silver\"]}, {\"id\": \"bike:2\", \"model\": \"Quaoar\", \"description\": \"Redesigned for the 2020 model year, this bike impressed our testers and is the best all-around trail bike we've ever tested. The Shimano gear system effectively does away with an external cassette, so is super low maintenance in terms of wear and tear. All in all it's an impressive package for the price, making it very competitive.\", \"price\": 2072, \"specs\": {\"material\": \"aluminium\", \"weight\": 7.9}, \"colors\": [\"black\", \"white\"]}, {\"id\": \"bike:3\", \"model\": \"Weywot\", \"description\": \"This bike gives kids aged six years and older a durable and uberlight mountain bike for their first experience on tracks and easy cruising through forests and fields. A set of powerful Shimano hydraulic disc brakes provide ample stopping ability. If you're after a budget option, this is one of the best bikes you could get.\", \"price\": 3264, \"specs\": {\"material\": \"alloy\", \"weight\": 13.8}}], \"commuter_bikes\": [{\"id\": \"bike:4\", \"model\": \"Salacia\", \"description\": \"This bike is a great option for anyone who just wants a bike to get about on With a slick-shifting Claris gears from Shimano\\u2019s, this is a bike which doesn\\u2019t break the bank and delivers craved performance.  It\\u2019s for the rider who wants both efficiency and capability.\", \"price\": 1475, \"specs\": {\"material\": \"aluminium\", \"weight\": 16.6}, \"colors\": [\"black\", \"silver\"]}, {\"id\": \"bike:5\", \"model\": \"Mimas\", \"description\": \"A real joy to ride, this bike got very high scores in last years Bike of the year report. The carefully crafted 50-34 tooth chainset and 11-32 tooth cassette give an easy-on-the-legs bottom gear for climbing, and the high-quality Vittoria Zaffiro tires give balance and grip.It includes a low-step frame , our memory foam seat, bump-resistant shocks and conveniently placed thumb throttle. Put it all together and you get a bike that helps redefine what can be done for this price.\", \"price\": 3941, \"specs\": {\"material\": \"alloy\", \"weight\": 11.6}}]}"
{{ /clients-example }}

#### Access JSON examples

The following examples use the `JSON.GET` command to retrieve data from various paths in the JSON document.

You can use the wildcard operator `*` to return a list of all items in the inventory:

{{< clients-example json_tutorial get_bikes >}}
> JSON.GET "bikes:inventory" $.inventory.*
"[[{\"id\":\"bike:1\",\"model\":\"Phoebe\",\"description\":\"This is a mid-travel trail slayer that is a fantastic daily driver or one bike quiver. The Shimano Claris 8-speed groupset gives plenty of gear range to tackle hills and there\xe2\x80\x99s room for mudguards and a rack too.  This is the bike for the rider who wants trail manners with low fuss ownership.\",\"price\":1920,\"specs\":{\"material\":\"carbon\",\"weight\":13.1},\"colors\":[\"black\",\"silver\"]},{\"id\":\"bike:2\",\"model\":\"Quaoar\",\"description\":\"Redesigned for the 2020 model year, this bike impressed our testers and is the best all-around trail bike we've ever tested. The Shimano gear system effectively does away with an external cassette, so is super low maintenance in terms of wear and tear. All in all it's an impressive package for the price, making it very competitive.\",\"price\":2072,\"specs\":{\"material\":\"aluminium\",\"weight\":7.9},\"colors\":[\"black\",\"white\"]},{\"id\":\"bike:3\",\"model\":\"Weywot\",\"description\":\"This bike gives kids aged six years and older a durable and uberlight mountain bike for their first experience on tracks and easy cruising through forests and fields. A set of powerful Shimano hydraulic disc brakes provide ample stopping ability. If you're after a budget option, this is one of the best bikes you could get.\",\"price\":3264,\"specs\":{\"material\":\"alloy\",\"weight\":13.8}}],[{\"id\":\"bike:4\",\"model\":\"Salacia\",\"description\":\"This bike is a great option for anyone who just wants a bike to get about on With a slick-shifting Claris gears from Shimano\xe2\x80\x99s, this is a bike which doesn\xe2\x80\x99t break the bank and delivers craved performance.  It\xe2\x80\x99s for the rider who wants both efficiency and capability.\",\"price\":1475,\"specs\":{\"material\":\"aluminium\",\"weight\":16.6},\"colors\":[\"black\",\"silver\"]},{\"id\":\"bike:5\",\"model\":\"Mimas\",\"description\":\"A real joy to ride, this bike got very high scores in last years Bike of the year report. The carefully crafted 50-34 tooth chainset and 11-32 tooth cassette give an easy-on-the-legs bottom gear for climbing, and the high-quality Vittoria Zaffiro tires give balance and grip.It includes a low-step frame , our memory foam seat, bump-resistant shocks and conveniently placed thumb throttle. Put it all together and you get a bike that helps redefine what can be done for this price.\",\"price\":3941,\"specs\":{\"material\":\"alloy\",\"weight\":11.6}}]]"
{{ /clients-example }}

For some queries, multiple paths can produce the same results. For example, the following paths return the models of mountain bikes:

{{< clients-example json_tutorial get_mtnbikes >}}
> JSON.GET "bikes:inventory" $.inventory.mountain_bikes[*].model
"[\"Phoebe\",\"Quaoar\",\"Weywot\"]"
> JSON.GET "bikes:inventory" '$.inventory["mountain_bikes"][*].model'
"[\"Phoebe\",\"Quaoar\",\"Weywot\"]"
> JSON.GET "bikes:inventory" $..mountain_bikes[*].model
"[\"Phoebe\",\"Quaoar\",\"Weywot\"]"
{{ /clients-example }}

The recursive descent operator `..` can retrieve a field from multiple sections of a JSON document. The following example returns the model of all inventory items:

{{< clients-example json_tutorial get_models >}}
> JSON.GET "bikes:inventory" $..model
"[\"Phoebe\",\"Quaoar\",\"Weywot\",\"Salacia\",\"Mimas\"]"
{{ /clients-example }}

You can use an array slice to select a range of elements from an array. This example returns the models of the first two mountain bikes:

{{< clients-example json_tutorial get2mtnbikes >}}
> JSON.GET "bikes:inventory" $..mountain_bikes[0:2].model
"[\"Phoebe\",\"Quaoar\"]"
{{ /clients-example }}

Filter expressions `?()` let you select JSON elements based on certain conditions. You can use comparison operators (`==`, `!=`, `<`, `<=`, `>`, `>=`, and starting with version v2.4.2, also `=~`), logical operators (`&&`, `||`), and parenthesis (`(`, `)`) within these expressions. A filter expression can be applied on an array or on an object, iterating over all the **elements** in the array or all the **values** in the object, retrieving only the ones that match the filter condition. 

Paths within the filter condition are using the dot notation with either `@` to denote the current array element or the current object value, or `$` to denote the top-level element. For example, use `@.key_name` to refer to a nested value and `$.top_level_key_name` to refer to a top-level value.

Starting with version v2.4.2, the comparison operator `=~` can be used for matching a path of a string value on the left side against a regular expression pattern on the right side. For more information, see the [supported regular expression syntax docs](https://docs.rs/regex/latest/regex/#syntax).

Non-string values do not match. A match can only occur when the left side is a path of a string value and the right side is either a hard-coded string, or a path of a string value. See [examples](#json-filter-examples) below.

The regex match is partial, meaning `"foo"` regex pattern matches a string such as `"barefoots"`.
To make it exact, use the regex pattern `"^foo$"`.

Other JSONPath engines may use regex pattern between slashes, e.g., `/foo/`, and their match is exact.
They can perform partial matches using a regex pattern such as `/.*foo.*/`.

#### JSON Filter examples

In the following examples, the filter only returns 
1 - mountain bikes with a price less than 3,000 and a weight less than 10
2 - the names of bikes made from an alloy
3 - starting with version v2.4.2 this will return only bikes whose description has the word 'easy' somewhere in it. Notice this match is case-insensitive thanks to the prefix `(?i)` in the regular expression pattern `"(?i)easy"`:

{{< clients-example json_tutorial filters >}}
> JSON.GET "bikes:inventory" "$..mountain_bikes[?(@.price < 3000 && @.specs.weight < 10)]"
"[{\"id\":\"bike:2\",\"model\":\"Quaoar\",\"description\":\"Redesigned for the 2020 model year, this bike impressed our testers and is the best all-around trail bike we've ever tested. The Shimano gear system effectively does away with an external cassette, so is super low maintenance in terms of wear and tear. All in all it's an impressive package for the price, making it very competitive.\",\"price\":2072,\"specs\":{\"material\":\"aluminium\",\"weight\":7.9},\"colors\":[\"black\",\"white\"]}]"

> JSON.GET "bikes:inventory" "$..[?(@.specs.material == 'alloy')].model"
"[\"Weywot\",\"Mimas\"]"

> JSON.GET "bikes:inventory" "$..[?(@.description =~ '(?i)easy')].model"
"[\"Quaoar\",\"Weywot\",\"Salacia\",\"Mimas\"]"
{{ /clients-example }}

#### Update JSON examples

You can also use JSONPath queries when you want to update specific sections of a JSON document.

For example, you can pass a JSONPath to the `JSON.NUMINCRBY` command to update a specific field. This example removes 100 to the price of each bike for a temporary sale, then adds it back:

{{< clients-example json_tutorial update_bikes >}}
> JSON.GET "bikes:inventory" $..price
"[1920,2072,3264,1475,3941]"
> JSON.NUMINCRBY "bikes:inventory" $..price -100
"[1820,1972,3164,1375,3841]"
> JSON.NUMINCRBY "bikes:inventory" $..price 100
"[1920,2072,3264,1475,3941]"
{{ /clients-example }}

You can use filter expressions to update only JSON elements that match certain conditions. JSONPath queries also work with other JSON commands that accept a path as an argument. The following example adds a new color option for bikes that cost less thatn 2,000. Note that it will also only apply to bikes that already have a color list as `JSON.ARRAPPEND` will not create the colors field for the other entries.

{{< clients-example json_tutorial update_filters >}}
> JSON.ARRAPPEND "bikes:inventory" $.inventory.*[?(@.price<2000)].colors '"pink"'
1) (integer) 3
2) (integer) 3
> JSON.GET "bikes:inventory" $..[*].colors
"[[\"black\",\"silver\",\"pink\"],[\"black\",\"white\"],[\"black\",\"silver\",\"pink\"]]"
{{ /clients-example }}

## Legacy path syntax

RedisJSON v1 had the following path implementation. JSON v2 still supports this legacy path in addition to JSONPath.

Paths always begin at the root of a Redis JSON value. The root is denoted by a period character (`.`). For paths that reference the root's children, it is optional to prefix the path with the root.

Redis JSON supports both dot notation and bracket notation for object key access. The following paths refer to _mountain bikes_, which is a child of _inventory_ under the root:

*   `.inventory.mountain_bikes`
*   `inventory["mountain_bikes"]`
*   `['inventory']["mountain_bikes"]`

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
