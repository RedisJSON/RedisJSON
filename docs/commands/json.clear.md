Clears container values (Arrays/Objects), sets numeric values to `0`, sets string value to emptys, and sets boolean values to `false`.

Already cleared values are ignored: empty containers, zero numbers, empty strings, `false`, and `null`.

`path` defaults to root if not provided. Non-existing keys and paths are ignored.

@return

@integer-reply: specifically the number of values cleared.
