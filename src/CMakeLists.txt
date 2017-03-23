# these are archives for testing
add_library(object STATIC object.c path.c json_path.c ${RMUTIL_DIR}/vector.c ${RMUTIL_DIR}/alloc.c)

add_library(json_object STATIC json_object.c ${JSONSL_DIR}/jsonsl.c ${RMUTIL_DIR}/sds.c)
target_link_libraries(json_object object)

# the same needs to be built for the module with REDIS_MODULE_TARGET publicly defined
add_library(rmobject STATIC object.c path.c json_path.c ${RMUTIL_DIR}/vector.c ${RMUTIL_DIR}/alloc.c)
target_compile_definitions(rmobject PUBLIC REDIS_MODULE_TARGET)

add_library(rmjson_object STATIC json_object.c ${JSONSL_DIR}/jsonsl.c ${RMUTIL_DIR}/sds.c)
target_compile_definitions(rmjson_object PUBLIC REDIS_MODULE_TARGET)
target_link_libraries(rmjson_object rmobject)

# prepare the configuration file
configure_file("config.h.in" "${PROJECT_BINARY_DIR}/config.h") 
include_directories("${PROJECT_BINARY_DIR}")

# the module itself
add_library(rejson SHARED rejson.c object_type.c json_type.c ${RMUTIL_DIR}/util.c)
set_target_properties(rejson PROPERTIES PREFIX "" C_VISIBILITY_PRESET hidden LINK_FLAGS "-Bsymbolic")
target_compile_definitions(rejson PUBLIC REDIS_MODULE_TARGET)
target_link_libraries(rejson rmjson_object m)
