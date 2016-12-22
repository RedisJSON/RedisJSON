#!/bin/bash
# unit testing script for json parser

PREFIX=$1
SUCCESS=$2
FILES=../test/validatortron-files
ERRCODE=0

for f in $( find $FILES -iname $PREFIX*.json ); do
    echo -n "$f: "
    ./json_validator $f
    if [ $? -ne $SUCCESS ]
    then
        ERRCODE=1
    fi
done

exit $ERRCODE