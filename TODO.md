# ReJSON TODOs

---

# MVP milestone

This is what ReJSON (https://github.com/redislabsmodules/rejson) currently has ready for the MVP:

*  Code is under src/
*  Building with CMake
  -     Need to verify on OSX
  -     Currently does not have an `install` option - needed?
*  Documentation
  -     docs/commands.md
  -     docs/design.md ~ 30% done
  -     README.md ~ 85% done
    -   Missing about/what is ReJSON
    -   Some notes about performance
    -   Perhaps a Node.js example
  - Source code is about 90% documented
*  AGPLv3 license
*  Copyright

## Missing misc

1.  Peer review of project CTO->RnD/QA?
1.  Number overflows in number operations
1.  Something is printing "inf"

## Source code/style

1.  Review and standardize use of int/size_t/uint32...
1.  Improve/remove error reporting and logging in case of module internal errors

## Benchmarks

1.  Need to include a simple standalone "benchmark", either w/ redis-benchmark or not ~ 30% done, need to complete some suites and generate graphs from output

## Examples

TBD

1. A session token that also has a list of last seen times, what stack though
1. Node.js example perhaps

## Blog post

References:

*   [Parsing JSON Is A Minefield](http://seriot.ch/parsing_json.php)

---

# Post MVP

## Profiling

1.  Memory usage: implemented JSON.MEMORY, need to compile an automated reporting tool
1.  Performance with callgrind and/or gperftools

## Build/test

1.  Run https://github.com/nst/JSONTestSuite and report like http://seriot.ch/parsing_json.php
1.  Need a dependable cycle to check for memory leaks
1.  Once we have a way to check baseline performance, add regression
1.  Fuzz all module commands with a mix of keys paths and values
1.  Memory leaks suite to run
    `valgrind --tool=memcheck --suppressions=../redis/src/valgrind.sup ../redis/src/redis-server --loadmodule ./lib/rejson.so`
1.  Verify each command's syntax - need a YAML
1.  Add CI to repo?

## Path parsing

1.  Add array slice

## Dictionary optimiztions

Encode as trie over a certain size threshold to save memory and increase lookup performance. Alternatively, use a hash dictionary.

## Secondary indexing

Integrate with @dvirsky's `secondary` library.

## Schema

Support [JSON Schema](http://json-schema.org/).

JSON.SETSCHEMA <key> <json>  

Notes:
1. Could be replaced by a JSON.SET modifier
2. Indexing will be specified in the schema
3. Cluster needs to be taken into account as well

JSON.VALIDATE <schema_key> <json>  

## Expiry

JSON.EXPIRE <key> <path> <ttl>  

# Cache serialized objects

Manage a cache inside the module for frequently accessed object in order to avoid repeatative
serialization.

## KeyRef nodes

Add a node type that references a Redis key that is either a JSON data type or a regular Redis key.
The module's API can transparently support resolving referenced keys and querying the data in them.
KeyRefs in cluster mode will only be allowed if in the same slot.

Redis core data types can be mapped to flat (i.e. non-nested) JSON structure as follows:
* A Redis String is a JSON String (albeit some escaping may be needed)
* A List: a JSON Array of JSON Strings, where the indices are the identical
* A Hash: a JSON Object where the Hash fields are the Object's keys and the values are JSON Strings
* A Set: a JSON Object where the Set members are the keys and their values are always a JSON Null
* A Sorted Set: a JSON Array that is made from two elements:
  * A JSON Object where each key is a member and value is the score
  * A JSON Array of all members ordered by score in ascending order

## Compression

Compress (string only? entire objects?) values over a (configureable?) size threshold with zstd.

## Additions to API

JSON.STATS
Print statistics about encountered values, parsing performance and such

JSON.OBJSET <key> <path> <value>
An alias for 'JSON.SET'

JSON.COUNT <key> <path> <json-scalar>  
P: count JS: ? R: N/A  
Counts the number of occurances for scalar in the array

JSON.REMOVE <key> <path> <json-scalar> [count]  
P: builtin del JS: ? R: LREM (but also has a count and direction)  
Removes the first `count` occurances (default 1) of value from array. If index is negative,
traversal is reversed.

JSON.EXISTS <key> <path>  
P: in JS: ? R: HEXISTS/LINDEX  
Checks if path key or array index exists. Syntactic sugar for JSON.TYPE.

JSON.REVERSE <key> <path>  
P: reverse JS: ? R: N/A  
Reverses the array. Nice to have.

JSON.SORT <key> <path>  
P: sort JS: ? R: SORT  
Sorts the values in an array. Nice to have.
