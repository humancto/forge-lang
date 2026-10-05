/*
 * forge_plugin.h — the Forge native plugin ABI, version 1.
 *
 * This header is the normative definition of the ABI. The Rust SDK
 * (crates/forge-plugin/src/abi.rs) and the Forge host
 * (src/plugins/abi.rs) mirror it and pin the layout with tests.
 * Design and rationale: rfcs/0006-native-plugins.md.
 *
 * A plugin is a shared library exporting exactly these two symbols:
 *
 *     uint32_t forge_plugin_abi_version(void);        // return FORGE_PLUGIN_ABI_VERSION
 *     const ForgePlugin *forge_plugin_register(void);  // return a static descriptor
 *
 * Forge calls forge_plugin_abi_version() first and refuses the library,
 * without calling anything else, when the version differs.
 *
 * OWNERSHIP RULES
 *   1. Arguments are borrowed: `args` and everything reachable from it
 *      belong to Forge and are valid only until `call` returns. Copy what
 *      you keep; never write through them.
 *   2. Results belong to the plugin: Forge sets *out to FORGE_NULL, the
 *      plugin fills it, Forge copies it and then passes it to the
 *      descriptor's free_value() — always, for every call, success or
 *      error. Forge never frees plugin memory itself, so use any allocator.
 *   3. `call` returns FORGE_OK (result in *out) or FORGE_ERR (*out should
 *      be a FORGE_STRING error message; it becomes the Forge error text).
 *   4. Strings are UTF-8, NOT NUL-terminated (ptr + len). A buffer, array
 *      or object with len 0 may have ptr == NULL; len > 0 with NULL is an
 *      error.
 *   5. Never let an exception, longjmp or Rust panic cross the boundary.
 *   6. Functions may be called concurrently from several threads.
 *   7. The descriptor and every string it points to must stay valid for
 *      the life of the process. Forge never unloads a plugin.
 *
 * SECURITY: a plugin runs with the full privileges of the Forge process,
 * outside every Forge permission. Forge only loads one when the `ffi`
 * capability is granted (--allow-ffi[=PATHS]).
 */
#ifndef FORGE_PLUGIN_H
#define FORGE_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define FORGE_PLUGIN_ABI_VERSION 1u

/* Value tags (ForgeValue.tag). */
#define FORGE_NULL   0u
#define FORGE_BOOL   1u  /* as.i: 0 = false, non-zero = true             */
#define FORGE_INT    2u  /* as.i                                         */
#define FORGE_FLOAT  3u  /* as.f                                         */
#define FORGE_STRING 4u  /* as.buf: UTF-8                                */
#define FORGE_BYTES  5u  /* as.buf: raw bytes (Forge sees ints 0..255)   */
#define FORGE_ARRAY  6u  /* as.array                                     */
#define FORGE_OBJECT 7u  /* as.object: entries in order, UTF-8 keys      */

/* Status codes returned by ForgeFn. */
#define FORGE_OK  0
#define FORGE_ERR 1

/* Arity of a function that accepts any number of arguments. */
#define FORGE_VARIADIC (-1)

typedef struct ForgeValue ForgeValue;
typedef struct ForgeEntry ForgeEntry;

typedef struct ForgeBuf {
    uint8_t *ptr;
    size_t len;
} ForgeBuf;

typedef struct ForgeArray {
    ForgeValue *ptr;
    size_t len;
} ForgeArray;

typedef struct ForgeObject {
    ForgeEntry *ptr;
    size_t len;
} ForgeObject;

struct ForgeValue {
    uint32_t tag;
    union {
        int64_t i;
        double f;
        ForgeBuf buf;
        ForgeArray array;
        ForgeObject object;
    } as;
};

struct ForgeEntry {
    ForgeBuf key;
    ForgeValue value;
};

/* One exported function: `name` must be a Forge identifier, unique within
 * the plugin. Forge checks the argument count before calling. */
typedef int32_t (*ForgeFn)(const ForgeValue *args, size_t argc, ForgeValue *out);

typedef struct ForgeFunction {
    const char *name;  /* NUL-terminated UTF-8                       */
    int32_t arity;     /* exact argument count, or FORGE_VARIADIC    */
    ForgeFn call;
} ForgeFunction;

typedef struct ForgePlugin {
    uint32_t abi_version;               /* FORGE_PLUGIN_ABI_VERSION       */
    const char *name;                   /* NUL-terminated UTF-8           */
    const char *version;                /* NUL-terminated UTF-8           */
    const ForgeFunction *functions;
    size_t function_count;
    void (*free_value)(ForgeValue *value); /* releases a returned value   */
} ForgePlugin;

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L && UINTPTR_MAX == 0xFFFFFFFFFFFFFFFFu
/* The 64-bit layout the Rust mirrors assert. */
_Static_assert(sizeof(ForgeValue) == 24, "ForgeValue layout");
_Static_assert(sizeof(ForgeEntry) == 40, "ForgeEntry layout");
_Static_assert(sizeof(ForgeFunction) == 24, "ForgeFunction layout");
_Static_assert(sizeof(ForgePlugin) == 48, "ForgePlugin layout");
#endif

/* Convenience constructors for plugin authors (results only). */
static inline ForgeValue forge_null(void) {
    ForgeValue v;
    v.tag = FORGE_NULL;
    v.as.i = 0;
    return v;
}

static inline ForgeValue forge_bool(int b) {
    ForgeValue v;
    v.tag = FORGE_BOOL;
    v.as.i = b ? 1 : 0;
    return v;
}

static inline ForgeValue forge_int(int64_t i) {
    ForgeValue v;
    v.tag = FORGE_INT;
    v.as.i = i;
    return v;
}

static inline ForgeValue forge_float(double f) {
    ForgeValue v;
    v.tag = FORGE_FLOAT;
    v.as.f = f;
    return v;
}

#ifdef __cplusplus
}
#endif

#endif /* FORGE_PLUGIN_H */
