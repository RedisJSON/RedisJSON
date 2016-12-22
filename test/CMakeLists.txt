# object functionality tests
add_executable(test_object test_object.c)
target_link_libraries(test_object object)
add_test(test_object test_object)

# JSON object functionality tests
add_executable(json_printer json_printer.c)
target_link_libraries(json_printer json_object m)
add_executable(test_json_object test_json_object.c)
target_link_libraries(test_json_object json_object m)
add_executable(json_validator json_validator.c)
target_link_libraries(json_validator json_object m)
add_test(test_json_object test_json_object)
add_test(NAME valid_json_files COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/json-validatortron.sh valid 0)
add_test(NAME invalid_json_files COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/json-validatortron.sh invalid 1)

# Module tests
# The path to the redis-server server is needed for test the module
set(REDIS_SERVER_PATH ${PROJECT_SOURCE_DIR}/../redis/src)
if(REDIS_SERVER_PATH AND EXISTS ${REDIS_SERVER_PATH}/redis-server)
    add_test(NAME redis_module COMMAND ${CMAKE_MAKE_PROGRAM} -C ${CMAKE_CURRENT_SOURCE_DIR}/pytest)
    set_property(TEST redis_module PROPERTY ENVIRONMENT 
        "REDIS_MODULE_PATH=${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/rejson.so;"
        "REDIS_SERVER_PATH=${REDIS_SERVER_PATH}/redis-server")
else()
    set(ERR_NO_REDIS
        "CMake 'REDIS_SERVER_PATH' variable undefined or path does not exist")
    message("!! ${ERR_NO_REDIS} - module unit test will not be run")
    add_test(NAME redis_module COMMAND echo "${ERR_NO_REDIS}")
    set_property(TEST redis_module PROPERTY WILL_FAIL true)
endif()