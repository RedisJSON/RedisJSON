cmake_minimum_required(VERSION 3.0.0)
project(ReJSON VERSION 0.0.0)

set(CMAKE_C_FLAGS_DEBUG "${CMAKE_C_FLAGS_DEBUG} -O0")

# Use gnu99, source: http://stackoverflow.com/a/30564223/3160475 
if (CMAKE_VERSION VERSION_LESS "3.1")
  if (CMAKE_C_COMPILER_ID STREQUAL "GNU")
    set (CMAKE_C_FLAGS "--std=gnu99 ${CMAKE_C_FLAGS}")
  endif ()
else ()
  set (CMAKE_C_STANDARD 99)
endif ()

# source: https://cmake.org/pipermail/cmake/2007-May/014345.html
# KDE's with -fPIC
IF(UNIX AND NOT WIN32)
  FIND_PROGRAM(CMAKE_UNAME uname /bin /usr/bin /usr/local/bin )
  IF(CMAKE_UNAME)
    EXEC_PROGRAM(uname ARGS -m OUTPUT_VARIABLE CMAKE_SYSTEM_PROCESSOR)
    SET(CMAKE_SYSTEM_PROCESSOR ${CMAKE_SYSTEM_PROCESSOR} CACHE INTERNAL
"processor type (i386 and x86_64)")
    IF(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64")
      ADD_DEFINITIONS(-fPIC)
    ENDIF(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64")
  ENDIF(CMAKE_UNAME)
ENDIF(UNIX AND NOT WIN32)

set (RMUTIL_DIR "${PROJECT_SOURCE_DIR}/deps/RedisModuleSDK/rmutil")
set (JSONSL_DIR "${PROJECT_SOURCE_DIR}/deps/jsonsl")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${PROJECT_SOURCE_DIR}/lib)

include(CTest)
enable_testing()

# This helps https://github.com/vector-of-bool/vscode-cmake-tools
include(CMakeToolsHelpers OPTIONAL)

include_directories("${PROJECT_SOURCE_DIR}/src" ${RMUTIL_DIR} ${JSONSL_DIR})
add_subdirectory(src)
add_subdirectory(test)

set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION ${PROJECT_VERSION})

include(CPack)
include_directories()