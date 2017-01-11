# object tests
add_executable(test_object test_object.c)
target_link_libraries(test_object object rt)
add_test(test_object test_object)

# JSON object tests
add_executable(json_printer json_printer.c)
target_link_libraries(json_printer json_object m)
add_executable(test_json_object test_json_object.c)
target_link_libraries(test_json_object json_object m rt)
add_test(test_json_object test_json_object)

# Test against JSON files
add_executable(json_validator json_validator.c)
target_link_libraries(json_validator json_object m)

set(TEST_FILES_PATH "${CMAKE_CURRENT_SOURCE_DIR}/files")
file(GLOB PASS "${TEST_FILES_PATH}/pass-*.json")

foreach(FILE ${PASS})
    get_filename_component(FNAME ${FILE} NAME)
    add_test(NAME json_validator:${FNAME} COMMAND json_validator ${FILE})
endforeach()

file(GLOB FAIL "${TEST_FILES_PATH}/fail-*.json")
foreach(FILE ${FAIL})
    get_filename_component(FNAME ${FILE} NAME)
    add_test(NAME json_validator:${FNAME} COMMAND json_validator ${FILE})
    set_property(TEST json_validator:${FNAME} PROPERTY WILL_FAIL true)
endforeach()

# Module test
# The full path to the redis-server executable is needed for testing the module
# You can provide it to cmake with: `cmake -DREDIS_SERVER_PATH=/path/to/redis-server --build build`
if (NOT REDIS_SERVER_PATH)
    set(ERR_NO_REDIS "'REDIS_SERVER_PATH' variable not defined (use cmake -D)")
    message("!! ${ERR_NO_REDIS} - module unit test will not be run")
    add_test(NAME redis_module COMMAND echo "${ERR_NO_REDIS} - test not run")
    set_property(TEST redis_module PROPERTY WILL_FAIL true)
elseif (NOT EXISTS ${REDIS_SERVER_PATH})
    set(ERR_NO_REDIS "${REDIS_SERVER_PATH} is not a valid 'REDIS_SERVER_PATH'")
    message("!! ${ERR_NO_REDIS} - module unit test will not be run")
    add_test(NAME redis_module COMMAND echo "${ERR_NO_REDIS} - test not run")
    set_property(TEST redis_module PROPERTY WILL_FAIL true)
elseif(REDIS_SERVER_PATH AND EXISTS ${REDIS_SERVER_PATH})
    add_test(NAME redis_module COMMAND ${CMAKE_MAKE_PROGRAM} -C ${CMAKE_CURRENT_SOURCE_DIR}/pytest)
    set_property(TEST redis_module PROPERTY ENVIRONMENT 
        "REDIS_MODULE_PATH=${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/rejson.so;"
        "REDIS_SERVER_PATH=${REDIS_SERVER_PATH};"
        "JSON_PATH=${PROJECT_SOURCE_DIR}/test/files")
endif()
