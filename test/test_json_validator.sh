#!/bin/bash
#
# Performs validation against use case files

# These files should pass validation
PASS_FILES=files/pass*
# These files should fail validation
FAIL_FILES=files/fail*

function munch {
    # Expects a list of JSON files and the expect result (0 pass, other fail)

    # Number of "failures"
    REPLY=0

    for f in $1
    do
        ./json_validator.out $f > /dev/null
        if [ $? -eq 0 ] # Validation passed?
        then
            if [ $2 -eq 0 ] # Expecting to pass?
            then
                echo -n "."
            else
                echo "E"
                echo "$f: expected to PASS, but FAILed"
                REPLY=$((REPLY+1))
            fi
        else
            if [ $2 -eq 0 ] # Expecting to pass?
            then
                echo "E"
                echo "$f: expected to FAIL, but PASSed"
                REPLY=$((REPLY+1))
            else
                echo -n "."
            fi
        fi
    done
    return $REPLY
}

echo
echo -n "Should PASS: "
munch "$PASS_FILES" 0
PASS_COUNT=$?

echo
echo -n "Should FAIL: "
munch "$FAIL_FILES" 1
FAIL_COUNT=$?

echo
echo
PASS_FILES=( $PASS_FILES )
FAIL_FILES=( $FAIL_FILES )
TOTAL_FILES=$((${#PASS_FILES[@]} + ${#FAIL_FILES[@]}))
TOTAL_COUNT=$(($PASS_COUNT + $FAIL_COUNT))
echo "$TOTAL_FILES JSON files validated, $TOTAL_COUNT problems detected"

exit $TOTAL_COUNT