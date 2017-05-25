# ReJSON Path

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
offsets can also be negative numbers, indicating indices starting at the end of the array. For
example, -1 is the last element in the array, -2 the penultimate, and so on.

## A note about JSON and path compatability

By definition, a JSON key can be any valid JSON String. Paths, on the other hand, are traditionally
based on JavaScript's (and in Java in turn) variable naming conventions. Therefore, while it is
possible to have ReJSON store objects containing arbitrary key names, accessing these keys via a
path will only be possible if they respect these naming syntax rules:

1.  Names must begin with a letter, a dollar (`$`) or an underscore (`_`) character
2.  Names can contain letters, digits, dollar signs and underscores
3.  Names are case-sensitive

## Time complexity of path evaluation

The complexity of searching (navigating to) an element in the path is made of:

1. Child level - every level along the path adds an additional search
2. Key search - O(N)<sup>&#8224;</sup>, where N is the number of keys in the parent object
3. Array search - O(1)

This means that the overall time complexity of searching a path is _O(N*M)_, where N is the depth
and M is the number of parent object keys.

<sup>&#8224;</sup> while this is acceptable for objects where N is small, access can
be optimized for larger objects, and this is planned for a future version.
