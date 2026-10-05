/*
 * A Forge native plugin in plain C.
 *
 *   cc -shared -fPIC -I ../../../crates/forge-plugin/include hello.c -o libhello_c.so
 *   (macOS: -o libhello_c.dylib)
 *   forge --allow-ffi run main.fg
 */
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#include "forge_plugin.h"

/* Results are allocated with malloc and released in hello_free below
 * (ownership rule 2). */
static ForgeValue make_string(const char *data, size_t len) {
    ForgeValue v = forge_null();
    uint8_t *copy = len ? (uint8_t *)malloc(len) : NULL;
    if (len && !copy) {
        return v;
    }
    if (len) {
        memcpy(copy, data, len);
    }
    v.tag = FORGE_STRING;
    v.as.buf.ptr = copy;
    v.as.buf.len = len;
    return v;
}

static int32_t fail(ForgeValue *out, const char *message) {
    *out = make_string(message, strlen(message));
    return FORGE_ERR;
}

/* mul(a: int, b: int) -> int */
static int32_t hello_mul(const ForgeValue *args, size_t argc, ForgeValue *out) {
    (void)argc; /* Forge checked the arity (2). */
    if (args[0].tag != FORGE_INT || args[1].tag != FORGE_INT) {
        return fail(out, "mul() expects two ints");
    }
    *out = forge_int(args[0].as.i * args[1].as.i);
    return FORGE_OK;
}

/* shout(text: string) -> string, upper-cased ASCII */
static int32_t hello_shout(const ForgeValue *args, size_t argc, ForgeValue *out) {
    (void)argc;
    if (args[0].tag != FORGE_STRING) {
        return fail(out, "shout() expects a string");
    }
    /* Arguments are borrowed and not NUL-terminated: copy with the length. */
    ForgeValue v = make_string((const char *)args[0].as.buf.ptr, args[0].as.buf.len);
    for (size_t i = 0; i < v.as.buf.len; i++) {
        v.as.buf.ptr[i] = (uint8_t)toupper(v.as.buf.ptr[i]);
    }
    *out = v;
    return FORGE_OK;
}

/* count(...) -> int: variadic, returns how many arguments it got */
static int32_t hello_count(const ForgeValue *args, size_t argc, ForgeValue *out) {
    (void)args;
    *out = forge_int((int64_t)argc);
    return FORGE_OK;
}

/* checked_div(a: int, b: int) -> int, or an error on division by zero */
static int32_t hello_checked_div(const ForgeValue *args, size_t argc, ForgeValue *out) {
    (void)argc;
    if (args[0].tag != FORGE_INT || args[1].tag != FORGE_INT) {
        return fail(out, "checked_div() expects two ints");
    }
    if (args[1].as.i == 0) {
        return fail(out, "checked_div(): division by zero");
    }
    *out = forge_int(args[0].as.i / args[1].as.i);
    return FORGE_OK;
}

static void hello_free(ForgeValue *value) {
    if (value && (value->tag == FORGE_STRING || value->tag == FORGE_BYTES)) {
        free(value->as.buf.ptr);
    }
    if (value) {
        *value = forge_null();
    }
}

static const ForgeFunction FUNCTIONS[] = {
    {"mul", 2, hello_mul},
    {"shout", 1, hello_shout},
    {"count", FORGE_VARIADIC, hello_count},
    {"checked_div", 2, hello_checked_div},
};

static const ForgePlugin PLUGIN = {
    FORGE_PLUGIN_ABI_VERSION,
    "hello_c",
    "0.1.0",
    FUNCTIONS,
    sizeof(FUNCTIONS) / sizeof(FUNCTIONS[0]),
    hello_free,
};

#if defined(_WIN32)
#define FORGE_EXPORT __declspec(dllexport)
#else
#define FORGE_EXPORT __attribute__((visibility("default")))
#endif

FORGE_EXPORT uint32_t forge_plugin_abi_version(void) { return FORGE_PLUGIN_ABI_VERSION; }

FORGE_EXPORT const ForgePlugin *forge_plugin_register(void) { return &PLUGIN; }
