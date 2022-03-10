---
title: "Commands"
linkTitle: "Commands"
type: docs
weight: 1
description: >
    Details relevant to all commands
---
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

Commands usually require a key's name as their first argument. The [path](/redisjson/path) is generally assumed to be the root if not specified.

The time complexity of the command does not include that of the [path](/redisjson/path#time-complexity-of-path-evaluation). The size - usually denoted _N_ - of a value is:

*   1 for scalar values
*   The sum of sizes of items in a container

## Scalar commands

## Array commands

## Object commands

## Module commands
