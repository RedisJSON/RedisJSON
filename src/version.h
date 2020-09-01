#ifndef REJSON_VERSION_H_
// This is where the modules build/version is declared.
// If declared with -D in compile time, this file is ignored


#ifndef REJSON_VERSION_MAJOR
#define REJSON_VERSION_MAJOR 1
#endif

#ifndef REJSON_VERSION_MINOR
#define REJSON_VERSION_MINOR 0
#endif

#ifndef REJSON_VERSION_PATCH
#define REJSON_VERSION_PATCH 5
#endif

#define REJSON_MODULE_VERSION \
  (REJSON_VERSION_MAJOR * 10000 + REJSON_VERSION_MINOR * 100 + REJSON_VERSION_PATCH)

#endif
