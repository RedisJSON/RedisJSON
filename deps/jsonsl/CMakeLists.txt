# This is here mainly to support Win32
CMAKE_MINIMUM_REQUIRED(VERSION 2.8.9)
ADD_DEFINITIONS(-DJSONSL_STATE_GENERIC)
IF(MSVC)
    ADD_DEFINITIONS(-D_CRT_SECURE_NO_WARNINGS)
    SET(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} /W3")
    SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} /W3")
ELSE()
    SET(jsonsl_cpp_warnings "-pedantic -Wall -Wextra -Wno-missing-field-initializers -Wno-unused-parameter")
    SET(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -std=c89 ${jsonsl_cpp_warnings}")
    SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++03 ${jsonsl_cpp_warnings}")
ENDIF()
INCLUDE_DIRECTORIES(${CMAKE_CURRENT_SOURCE_DIR})
ADD_LIBRARY(jsonsl jsonsl.c)
EXECUTE_PROCESS(
    COMMAND
        ${CMAKE_COMMAND} -E tar xzf ${CMAKE_CURRENT_SOURCE_DIR}/json_samples.tgz
    WORKING_DIRECTORY
        ${CMAKE_CURRENT_BINARY_DIR})


ENABLE_TESTING()
ADD_SUBDIRECTORY(tests)

# Add the benchmarks:
ADD_EXECUTABLE(bench-simple EXCLUDE_FROM_ALL perf/bench.c jsonsl.c)
ADD_EXECUTABLE(yajl-perftest EXCLUDE_FROM_ALL perf/documents.c perf/perftest.c jsonsl.c)
IF(CMAKE_MAJOR_VERSION GREATER 2 OR CMAKE_MINOR_VERSION GREATER 8)
    ADD_CUSTOM_TARGET(bench
        COMMAND $<TARGET_FILE:bench-simple> ${CMAKE_CURRENT_BINARY_DIR}/share/auction 100
        COMMAND $<TARGET_FILE:yajl-perftest>
        WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})
ENDIF()
