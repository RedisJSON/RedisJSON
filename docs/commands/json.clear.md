Clears container values (Arrays/Objects), and sets numeric values to `0`.

Already cleared values are ignored: empty containers, and zero numbers.

`path` defaults to root if not provided. Non-existing paths are ignored.

@return

@integer-reply: specifically the number of values cleared.

@examples

```
redis> JSON.SET doc $ '{"obj":{"a":1, "b":2}, "arr":[1,2,3], "str": "foo", "bool": true, "int": 42, "float": 3.14}'
OK
redis> JSON.CLEAR doc $.*
(integer) 4
redis> JSON.GET doc $
"[{\"obj\":{},\"arr\":[],\"str\":\"foo\",\"bool\":true,\"int\":0,\"float\":0}]"
```
