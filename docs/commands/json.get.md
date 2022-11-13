Returns the value at `path` in JSON serialized form.

This command accepts multiple `path` arguments. If no path is given, it defaults to the value's root.

The following subcommands change the reply's format (all are empty string by default):

*   `INDENT` sets the indentation string for nested levels
*   `NEWLINE` sets the string that's printed at the end of each line
*   `SPACE` sets the string that's put between a key and a value
*   `FORMAT` sets result format the current supported formats JSON/BSON (Default JSON)

Produce pretty-formatted JSON with `redis-cli` by following this example:

```
~/$ redis-cli --raw
127.0.0.1:6379> JSON.GET myjsonkey INDENT "\t" NEWLINE "\n" SPACE " " path.to.value[1]
```

@return

@array-reply of @bulk-string-reply - each string is the JSON serialization of each JSON value that matches a path.

When using a JSONPath, the root of the matching values is always an array. In contrast, the legacy path returns a single value.

If there are multiple paths that include both legacy path and JSONPath, the returned value conforms to the JSONPath version (an array of values). 

@examples

```
redis> JSON.SET doc $ '{"a":2, "b": 3, "nested": {"a": 4, "b": null}}'
OK
```

With a single JSONPath (JSON array bulk string):

```
redis> JSON.GET doc $..b
"[3,null]"
```

Using multiple paths with at least one JSONPath (map with array of JSON values per path):

```
redis> JSON.GET doc ..a $..b
"{\"$..b\":[3,null],\"..a\":[2,4]}"
```
