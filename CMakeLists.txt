cmake_minimum_required(VERSION 3.0.0)
project(rejson VERSION 0.0.0)

include(CTest)
enable_testing()
add_executable(test_object test/test_object.c src/object.c)

add_test(test_object test_object)

add_library(rejson src/object.c)

set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION ${PROJECT_VERSION})
include(CPack)
